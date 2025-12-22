//! # Nika Error Module (v0)
//!
//! Unified error handling with fix suggestions for the Nika CLI.
//!
//! ## Design Philosophy (v0 = simplify)
//!
//! After analyzing the codebase, we chose NOT to consolidate error types because:
//!
//! 1. **Clear separation of concerns**:
//!    - `types.rs` errors: Domain validation (TaskId, URL, etc.)
//!    - `validator.rs` errors: 5-layer validation pipeline with NIKA-XXX codes
//!    - `runner/` errors: Runtime execution errors
//!    - `builders.rs` errors: Fluent builder API errors
//!
//! 2. **Each layer has different consumers**
//! 3. **thiserror already provides `std::error::Error`**
//!
//! Instead, this module provides:
//! - A `FixSuggestion` trait for unified fix suggestion access
//! - A `NikaError` enum for top-level CLI errors (wrapping specific errors)
//!
//! ## Example
//!
//! ```rust,ignore
//! use nika::error::{NikaError, FixSuggestion};
//!
//! fn handle_error(err: NikaError) {
//!     eprintln!("Error: {}", err);
//!     if let Some(suggestion) = err.fix_suggestion() {
//!         eprintln!("  Fix: {}", suggestion);
//!     }
//! }
//! ```

use std::fmt;
use thiserror::Error;

// ============================================================================
// FIX SUGGESTION TRAIT
// ============================================================================

/// Trait for errors that can provide fix suggestions
///
/// All Nika errors should implement this trait to provide helpful
/// guidance to users on how to resolve the error.
pub trait FixSuggestion {
    /// Get a fix suggestion for this error, if available
    fn fix_suggestion(&self) -> Option<&str>;
}

// ============================================================================
// NIKA ERROR (Top-level CLI Error)
// ============================================================================

/// Top-level error type for the Nika CLI
///
/// This wraps specific error types from different modules,
/// providing a unified interface for the CLI entry point.
#[derive(Error, Debug)]
pub enum NikaError {
    /// Validation pipeline error
    #[error("{0}")]
    Validation(#[from] crate::validator::ValidationError),

    /// Builder error
    #[error("{0}")]
    Builder(#[from] crate::builders::BuilderError),

    /// Agent execution error
    #[error("{0}")]
    Agent(#[from] crate::runner::core::AgentError),

    /// Task ID validation error
    #[error("{0}")]
    TaskId(#[from] crate::types::TaskIdError),

    /// Workflow name validation error
    #[error("{0}")]
    WorkflowName(#[from] crate::types::WorkflowNameError),

    /// Prompt validation error
    #[error("{0}")]
    Prompt(#[from] crate::types::PromptError),

    /// Model name validation error
    #[error("{0}")]
    ModelName(#[from] crate::types::ModelNameError),

    /// Shell command validation error
    #[error("{0}")]
    ShellCommand(#[from] crate::types::ShellCommandError),

    /// URL validation error
    #[error("{0}")]
    Url(#[from] crate::types::UrlError),

    /// YAML parsing error
    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    /// IO error (file not found, permission denied, etc.)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error with message
    #[error("{0}")]
    Other(String),
}

impl NikaError {
    /// Create a generic error with a message
    pub fn other(msg: impl Into<String>) -> Self {
        NikaError::Other(msg.into())
    }
}

impl FixSuggestion for NikaError {
    fn fix_suggestion(&self) -> Option<&str> {
        match self {
            NikaError::Validation(e) => e.fix_suggestion(),
            NikaError::Builder(e) => e.fix_suggestion(),
            NikaError::Agent(e) => e.fix_suggestion(),
            NikaError::TaskId(e) => e.fix_suggestion(),
            NikaError::WorkflowName(e) => e.fix_suggestion(),
            NikaError::Prompt(e) => e.fix_suggestion(),
            NikaError::ModelName(e) => e.fix_suggestion(),
            NikaError::ShellCommand(e) => e.fix_suggestion(),
            NikaError::Url(e) => e.fix_suggestion(),
            NikaError::YamlParse(_) => {
                Some("Check YAML syntax: ensure proper indentation and quoting")
            }
            NikaError::Io(_) => Some("Check file path exists and has correct permissions"),
            NikaError::Other(_) => None,
        }
    }
}

// ============================================================================
// FIX SUGGESTION IMPLEMENTATIONS
// ============================================================================

impl FixSuggestion for crate::validator::ValidationError {
    fn fix_suggestion(&self) -> Option<&str> {
        use crate::validator::ValidationError;

        Some(match self {
            ValidationError::MissingModel => {
                "Add 'model:' field to agent config. Example: model: claude-sonnet-4-5"
            }
            ValidationError::MissingSystemPrompt => {
                "Add 'systemPrompt:' or 'systemPromptFile:' to agent config"
            }
            ValidationError::TaskError { .. } => {
                "Check task configuration. Each task needs exactly one keyword (agent, subagent, shell, http, mcp, function, or llm)"
            }
            ValidationError::DuplicateTaskId { .. } => {
                "Use unique task IDs. Rename one of the duplicate tasks"
            }
            ValidationError::ToolAccessError { .. } => {
                "Ensure task tools are subset of agent.allowedTools pool, or use subagent: for independent tool access"
            }
            ValidationError::FlowError { .. } => {
                "Check that source and target task IDs exist and are different"
            }
            ValidationError::ConnectionBlocked { .. } => {
                "Use bridge pattern: subagent: -> function: -> agent: (subagent cannot directly connect to another subagent)"
            }
            ValidationError::GraphWarning { .. } => {
                "This is a warning. Consider connecting orphan tasks or removing them"
            }
        })
    }
}

impl FixSuggestion for crate::builders::BuilderError {
    fn fix_suggestion(&self) -> Option<&str> {
        use crate::builders::BuilderError;

        Some(match self {
            BuilderError::InvalidName(_) => {
                "Use alphanumeric characters, dashes, and underscores only"
            }
            BuilderError::InvalidId(_) => {
                "Task ID: use alphanumeric, dash (-), underscore (_). Max 64 chars"
            }
            BuilderError::MissingAgent => {
                "Call .with_agent() or .agent() before .build()"
            }
            BuilderError::MissingField(field) => {
                match field.as_str() {
                    "model" => "Set the model: .model(\"claude-sonnet-4-5\")",
                    _ => "Provide the required field before calling .build()",
                }
            }
            BuilderError::NoTasks => {
                "Add at least one task: .with_task(\"id\", |t| t.agent(\"prompt\"))"
            }
            BuilderError::NoKeyword => {
                "Set exactly one keyword: .agent(), .subagent(), .shell(), .http(), .mcp(), .function(), or .llm()"
            }
            BuilderError::MultipleKeywords => {
                "Remove extra keywords. Each task must have exactly one"
            }
            BuilderError::UnsafeCommand(_) => {
                "Use a safe command or use unsafe { .shell_unchecked() } if intentional"
            }
            BuilderError::InvalidUrl(_) => {
                "Use valid URL: https://example.com or http://localhost:8080"
            }
        })
    }
}

impl FixSuggestion for crate::runner::core::AgentError {
    fn fix_suggestion(&self) -> Option<&str> {
        use crate::runner::core::AgentError;

        Some(match self {
            AgentError::Provider(_) => "Check provider configuration and API credentials",
            AgentError::Template(_) => {
                "Check template syntax: use {{task_id}} for outputs, ${var} for inputs"
            }
            AgentError::Config(_) => {
                "Review agent configuration: model, systemPrompt, allowedTools"
            }
            AgentError::Timeout(_) => "Increase timeout in task config or simplify the prompt",
        })
    }
}

impl FixSuggestion for crate::types::TaskIdError {
    fn fix_suggestion(&self) -> Option<&str> {
        use crate::types::TaskIdError;

        Some(match self {
            TaskIdError::Empty => "Example: 'analyze', 'fetch-data', 'step_1'",
            TaskIdError::TooLong(_) => "Keep task IDs concise (max 64 chars)",
            TaskIdError::InvalidCharacters(_) => {
                "Replace special chars: 'my task' -> 'my-task', 'step.1' -> 'step-1'"
            }
        })
    }
}

impl FixSuggestion for crate::types::WorkflowNameError {
    fn fix_suggestion(&self) -> Option<&str> {
        use crate::types::WorkflowNameError;

        Some(match self {
            WorkflowNameError::Empty => "Example: 'code-review', 'data-pipeline', 'security-audit'",
            WorkflowNameError::TooLong(_) => "Keep workflow names concise (max 128 chars)",
        })
    }
}

impl FixSuggestion for crate::types::PromptError {
    fn fix_suggestion(&self) -> Option<&str> {
        use crate::types::PromptError;

        Some(match self {
            PromptError::Empty => "Provide a descriptive prompt for the task",
            PromptError::TooLong(_) => "Split into multiple tasks or use systemPromptFile",
        })
    }
}

impl FixSuggestion for crate::types::ModelNameError {
    fn fix_suggestion(&self) -> Option<&str> {
        use crate::types::ModelNameError;

        Some(match self {
            ModelNameError::Empty => {
                "Use: claude-sonnet-4-5, claude-opus-4, claude-haiku, gpt-4o, etc."
            }
            ModelNameError::InvalidCharacters(_) => {
                "Use alphanumeric, dash (-), underscore (_), dot (.) only"
            }
        })
    }
}

impl FixSuggestion for crate::types::ShellCommandError {
    fn fix_suggestion(&self) -> Option<&str> {
        use crate::types::ShellCommandError;

        Some(match self {
            ShellCommandError::Empty => "Example: 'npm test', 'cargo build', 'ls -la'",
            ShellCommandError::Dangerous(_) => {
                "This command is blocked for safety. Use a safer alternative or review the command"
            }
        })
    }
}

impl FixSuggestion for crate::types::UrlError {
    fn fix_suggestion(&self) -> Option<&str> {
        use crate::types::UrlError;

        Some(match self {
            UrlError::Empty => "Example: 'https://api.example.com/v1', 'http://localhost:8080'",
            UrlError::InvalidScheme(_) => "URL must start with http://, https://, or file://",
        })
    }
}

// ============================================================================
// DISPLAY HELPERS
// ============================================================================

/// Format an error with its fix suggestion for display
pub fn format_error_with_suggestion<E: std::error::Error + FixSuggestion>(error: &E) -> String {
    let mut result = error.to_string();
    if let Some(suggestion) = error.fix_suggestion() {
        result.push_str("\n  Fix: ");
        result.push_str(suggestion);
    }
    result
}

/// Print an error with its fix suggestion to stderr
pub fn print_error<E: std::error::Error + FixSuggestion>(error: &E) {
    eprintln!("Error: {}", error);
    if let Some(suggestion) = error.fix_suggestion() {
        eprintln!("  Fix: {}", suggestion);
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_has_suggestion() {
        let error = crate::validator::ValidationError::MissingModel;
        assert!(error.fix_suggestion().is_some());
        assert!(error.fix_suggestion().unwrap().contains("model"));
    }

    #[test]
    fn test_task_id_error_has_suggestion() {
        let error = crate::types::TaskIdError::Empty;
        assert!(error.fix_suggestion().is_some());
        assert!(error.fix_suggestion().unwrap().contains("analyze"));
    }

    #[test]
    fn test_builder_error_has_suggestion() {
        let error = crate::builders::BuilderError::NoKeyword;
        assert!(error.fix_suggestion().is_some());
    }

    #[test]
    fn test_nika_error_wraps_validation() {
        let validation_err = crate::validator::ValidationError::MissingModel;
        let nika_err: NikaError = validation_err.into();

        assert!(nika_err.to_string().contains("NIKA-001"));
        assert!(nika_err.fix_suggestion().is_some());
    }

    #[test]
    fn test_nika_error_wraps_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let nika_err: NikaError = io_err.into();

        assert!(nika_err.to_string().contains("IO error"));
        assert!(nika_err.fix_suggestion().is_some());
    }

    #[test]
    fn test_format_error_with_suggestion() {
        let error = crate::validator::ValidationError::MissingModel;
        let formatted = format_error_with_suggestion(&error);

        assert!(formatted.contains("NIKA-001"));
        assert!(formatted.contains("Fix:"));
        assert!(formatted.contains("model"));
    }

    #[test]
    fn test_all_validation_errors_have_suggestions() {
        use crate::validator::ValidationError;
        use crate::TaskCategory;

        let errors = vec![
            ValidationError::MissingModel,
            ValidationError::MissingSystemPrompt,
            ValidationError::TaskError {
                task_id: "test".into(),
                message: "test".into(),
            },
            ValidationError::DuplicateTaskId {
                task_id: "test".into(),
            },
            ValidationError::ToolAccessError {
                task_id: "test".into(),
                message: "test".into(),
            },
            ValidationError::FlowError {
                from_task: "a".into(),
                to_task: "b".into(),
                message: "test".into(),
            },
            ValidationError::ConnectionBlocked {
                from_task: "a".into(),
                from_key: TaskCategory::Isolated,
                to_task: "b".into(),
                to_key: TaskCategory::Isolated,
            },
            ValidationError::GraphWarning {
                message: "test".into(),
            },
        ];

        for error in errors {
            assert!(
                error.fix_suggestion().is_some(),
                "Missing fix suggestion for: {:?}",
                error
            );
        }
    }

    #[test]
    fn test_all_builder_errors_have_suggestions() {
        use crate::builders::BuilderError;

        let errors = vec![
            BuilderError::InvalidName("test".into()),
            BuilderError::InvalidId("test".into()),
            BuilderError::MissingAgent,
            BuilderError::MissingField("model".into()),
            BuilderError::NoTasks,
            BuilderError::NoKeyword,
            BuilderError::MultipleKeywords,
            BuilderError::UnsafeCommand("rm".into()),
            BuilderError::InvalidUrl("bad".into()),
        ];

        for error in errors {
            assert!(
                error.fix_suggestion().is_some(),
                "Missing fix suggestion for: {:?}",
                error
            );
        }
    }

    #[test]
    fn test_all_agent_errors_have_suggestions() {
        use crate::runner::core::AgentError;

        let errors = vec![
            AgentError::Provider("test".into()),
            AgentError::Template("test".into()),
            AgentError::Config("test".into()),
            AgentError::Timeout("test".into()),
        ];

        for error in errors {
            assert!(
                error.fix_suggestion().is_some(),
                "Missing fix suggestion for: {:?}",
                error
            );
        }
    }
}
