// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Domain-specific error types for ARCH-3 incremental migration.
//!
//! These sub-enums group NikaError variants by domain. Each implements
//! `From<SubEnum> for NikaError` so new code can use domain errors
//! while existing code keeps working unchanged.
//!
//! # Migration Path
//!
//! 1. (THIS COMMIT) Create domain enums + From impls
//! 2. New code uses `ProviderError::NotConfigured` instead of
//!    `NikaError::ProviderNotConfigured`
//! 3. Gradually migrate existing call sites per-module
//! 4. Eventually make NikaError variants delegate to domain enums
//!
//! # Domain Groups
//!
//! | Range | Domain | Enum |
//! |-------|--------|------|
//! | 001-009 | Workflow | `WorkflowError` |
//! | 010-019 | Schema | `SchemaError` |
//! | 020-029 | DAG | `DagError` |
//! | 030-039 | Provider | `ProviderError` |
//! | 040-049 | Binding/Template | `BindingError` |
//! | 050-059 | Path/Security | `SecurityError` |
//! | 060-069 | Output | `OutputError` |
//! | 090-099 | Execution | `ExecutionError` |
//! | 100-109 | MCP | `McpError` |
//! | 110-119 | Agent | `AgentError` |
//! | 200-219 | File/Builtin tools | `ToolError` |
//! | 250-299 | Media | `MediaError` |
//! | 300-319 | Structured output | `StructuredOutputError` |

use crate::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// PROVIDER ERRORS (030-039)
// ═══════════════════════════════════════════════════════════════════════════

/// Provider-related errors (API keys, connections, configuration).
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("[NIKA-030] Provider '{provider}' not configured")]
    NotConfigured { provider: String },

    #[error("[NIKA-031] Provider API error: {message}")]
    ApiError { message: String },

    #[error("[NIKA-032] Missing API key for provider '{provider}'")]
    MissingApiKey { provider: String },

    #[error("[NIKA-033] Invalid configuration: {message}")]
    InvalidConfig { message: String },

    #[error("[NIKA-035] Custom endpoint '{name}' not found in config -- add it to [endpoints.{name}] in ~/.config/nika/config.toml")]
    EndpointNotFound { name: String },

    #[error("[NIKA-036] Cannot connect to custom endpoint '{endpoint}': {reason}")]
    EndpointConnectionFailed { endpoint: String, reason: String },

    #[error("[NIKA-037] Fallback chain exhausted — all providers failed. Last: {last_provider}. Error: {last_error}")]
    FallbackChainExhausted {
        last_provider: String,
        last_error: String,
    },
}

impl From<ProviderError> for NikaError {
    fn from(e: ProviderError) -> Self {
        match e {
            ProviderError::NotConfigured { provider } => {
                NikaError::ProviderNotConfigured { provider }
            }
            ProviderError::ApiError { message } => NikaError::ProviderApiError { message },
            ProviderError::MissingApiKey { provider } => NikaError::MissingApiKey { provider },
            ProviderError::InvalidConfig { message } => NikaError::InvalidConfig { message },
            ProviderError::EndpointNotFound { name } => NikaError::EndpointNotFound { name },
            ProviderError::EndpointConnectionFailed { endpoint, reason } => {
                NikaError::EndpointConnectionFailed { endpoint, reason }
            }
            ProviderError::FallbackChainExhausted {
                last_provider,
                last_error,
            } => NikaError::FallbackChainExhausted {
                providers: last_provider,
                last_error,
            },
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DAG ERRORS (020-029)
// ═══════════════════════════════════════════════════════════════════════════

/// DAG structure errors (cycles, missing deps, duplicates).
#[derive(Debug, thiserror::Error)]
pub enum DagError {
    #[error("[NIKA-020] Cycle detected in DAG: {cycle}")]
    CycleDetected { cycle: String },

    #[error("[NIKA-021] Missing dependency: task '{task_id}' depends on unknown '{dep_id}'")]
    MissingDependency { task_id: String, dep_id: String },

    #[error("[NIKA-022] Duplicate task ID: '{task_id}' appears multiple times")]
    DuplicateTaskId { task_id: String },
}

impl From<DagError> for NikaError {
    fn from(e: DagError) -> Self {
        match e {
            DagError::CycleDetected { cycle } => NikaError::CycleDetected { cycle },
            DagError::MissingDependency { task_id, dep_id } => {
                NikaError::MissingDependency { task_id, dep_id }
            }
            DagError::DuplicateTaskId { task_id } => NikaError::DuplicateTaskId { task_id },
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EXECUTION ERRORS (044-046 exec/fetch/extract + 096-098 general/cancel/panic)
// ═══════════════════════════════════════════════════════════════════════════

/// Runtime execution errors.
#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("[NIKA-044] Exec error: {reason}")]
    ExecFailed { reason: String },

    #[error("[NIKA-045] Fetch error: {reason}")]
    FetchFailed { reason: String },

    #[error("[NIKA-046] Extract error: {reason}")]
    ExtractFailed { reason: String },

    #[error("[NIKA-096] Execution error: {0}")]
    General(String),

    #[error("[NIKA-097] Workflow cancelled: {phase}")]
    Cancelled { phase: String },

    #[error("[NIKA-098] Task panicked: {reason}")]
    Panicked { reason: String },
}

impl From<ExecutionError> for NikaError {
    fn from(e: ExecutionError) -> Self {
        match e {
            ExecutionError::ExecFailed { reason } => NikaError::ExecError { reason },
            ExecutionError::FetchFailed { reason } => NikaError::FetchError { reason },
            ExecutionError::ExtractFailed { reason } => NikaError::ExtractError { reason },
            ExecutionError::General(msg) => NikaError::Execution(msg),
            ExecutionError::Cancelled { phase } => NikaError::WorkflowCancelled { phase },
            ExecutionError::Panicked { reason } => NikaError::TaskPanicked { reason },
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BINDING ERRORS (040-049)
// ═══════════════════════════════════════════════════════════════════════════

/// Template and binding errors.
#[derive(Debug, thiserror::Error)]
pub enum BindingError {
    #[error("[NIKA-041] Template error in '{template}': {reason}")]
    TemplateError { template: String, reason: String },

    #[error("[NIKA-042] Binding '{alias}' not found")]
    NotFound { alias: String },

    #[error("[NIKA-043] Binding type mismatch at '{path}': expected {expected}, got {actual}")]
    TypeMismatch {
        path: String,
        expected: String,
        actual: String,
    },
}

impl From<BindingError> for NikaError {
    fn from(e: BindingError) -> Self {
        match e {
            BindingError::TemplateError { template, reason } => {
                NikaError::TemplateError { template, reason }
            }
            BindingError::NotFound { alias } => NikaError::BindingNotFound { alias },
            BindingError::TypeMismatch {
                path,
                expected,
                actual,
            } => NikaError::BindingTypeMismatch {
                path,
                expected,
                actual,
            },
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_error_converts_to_nika_error() {
        let err: NikaError = ProviderError::NotConfigured {
            provider: "anthropic".into(),
        }
        .into();
        assert!(err.to_string().contains("NIKA-030"));
        assert!(err.to_string().contains("anthropic"));
    }

    #[test]
    fn dag_error_converts_to_nika_error() {
        let err: NikaError = DagError::CycleDetected {
            cycle: "a → b → a".into(),
        }
        .into();
        assert!(err.to_string().contains("NIKA-020"));
    }

    #[test]
    fn execution_error_converts_to_nika_error() {
        let err: NikaError = ExecutionError::General("test error".into()).into();
        assert!(err.to_string().contains("NIKA-096"));
    }

    #[test]
    fn binding_error_converts_to_nika_error() {
        let err: NikaError = BindingError::NotFound {
            alias: "data".into(),
        }
        .into();
        assert!(err.to_string().contains("NIKA-042"));
    }

    #[test]
    fn domain_errors_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ProviderError>();
        assert_send_sync::<DagError>();
        assert_send_sync::<ExecutionError>();
        assert_send_sync::<BindingError>();
    }
}
