// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Schema error types — parse errors, analysis errors, validation errors.
//!
//! Code range: NIKA-280..329 (`Category::Schema`).

use nika_error::codes::{Category, NikaCode, Severity};
use nika_error::traits::NikaErrorCode;

use crate::source::Span;

/// Errors from the schema layer (parser, analyzer, validator).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum SchemaError {
    /// YAML syntax error.
    #[error("YAML parse error: {message}")]
    YamlSyntax {
        /// Description of the syntax error.
        message: String,
        /// Source span where the error occurred.
        span: Option<Span>,
    },

    /// Unknown or invalid task action verb.
    #[error("unknown action verb: {verb}")]
    UnknownVerb {
        /// The verb that was not recognized.
        verb: String,
        /// Source span.
        span: Option<Span>,
    },

    /// Missing required field in a task or workflow.
    #[error("missing required field: {field}")]
    MissingField {
        /// Name of the missing field.
        field: String,
        /// Source span.
        span: Option<Span>,
    },

    /// Invalid task identifier (not `snake_case` or reserved).
    #[error("invalid task ID: {id} — {reason}")]
    InvalidTaskId {
        /// The invalid task identifier.
        id: String,
        /// Why it's invalid.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },

    /// Duplicate task name in the same workflow.
    #[error("duplicate task name: {name}")]
    DuplicateTask {
        /// The duplicate name.
        name: String,
        /// Span of the duplicate.
        span: Option<Span>,
    },

    /// Dependency cycle detected in the task DAG.
    #[error("dependency cycle: {}", cycle.join(" → "))]
    Cycle {
        /// Task names forming the cycle.
        cycle: Vec<String>,
    },

    /// Reference to unknown task in `depends_on`.
    #[error("unknown dependency: task {from} depends on {to}, which does not exist")]
    UnknownDependency {
        /// The task that has the dependency.
        from: String,
        /// The referenced task that doesn't exist.
        to: String,
        /// Source span.
        span: Option<Span>,
    },

    /// Invalid structured output configuration.
    #[error("invalid structured output: {reason}")]
    InvalidStructuredOutput {
        /// Why the config is invalid.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },

    /// Guardrail configuration error.
    #[error("guardrail error: {reason}")]
    InvalidGuardrail {
        /// What's wrong with the guardrail.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },

    /// Schema version mismatch or unsupported version.
    #[error("unsupported schema version: {version}")]
    UnsupportedVersion {
        /// The version string.
        version: String,
    },

    /// Generic validation error (catch-all for rare cases).
    #[error("validation error: {message}")]
    Validation {
        /// Description.
        message: String,
        /// Source span.
        span: Option<Span>,
    },
}

// ─── NIKA-280..290 schema error codes ───────────────────────────────

const SCHEMA_280: NikaCode = NikaCode {
    num: 280,
    category: Category::Schema,
    severity: Severity::Error,
    slug: "yaml-syntax",
};
const SCHEMA_281: NikaCode = NikaCode {
    num: 281,
    category: Category::Schema,
    severity: Severity::Error,
    slug: "unknown-verb",
};
const SCHEMA_282: NikaCode = NikaCode {
    num: 282,
    category: Category::Schema,
    severity: Severity::Error,
    slug: "missing-field",
};
const SCHEMA_283: NikaCode = NikaCode {
    num: 283,
    category: Category::Schema,
    severity: Severity::Error,
    slug: "invalid-task-id",
};
const SCHEMA_284: NikaCode = NikaCode {
    num: 284,
    category: Category::Schema,
    severity: Severity::Error,
    slug: "duplicate-task",
};
const SCHEMA_285: NikaCode = NikaCode {
    num: 285,
    category: Category::Schema,
    severity: Severity::Error,
    slug: "cycle",
};
const SCHEMA_286: NikaCode = NikaCode {
    num: 286,
    category: Category::Schema,
    severity: Severity::Error,
    slug: "unknown-dependency",
};
const SCHEMA_287: NikaCode = NikaCode {
    num: 287,
    category: Category::Schema,
    severity: Severity::Error,
    slug: "invalid-structured-output",
};
const SCHEMA_288: NikaCode = NikaCode {
    num: 288,
    category: Category::Schema,
    severity: Severity::Error,
    slug: "invalid-guardrail",
};
const SCHEMA_289: NikaCode = NikaCode {
    num: 289,
    category: Category::Schema,
    severity: Severity::Error,
    slug: "unsupported-version",
};
const SCHEMA_290: NikaCode = NikaCode {
    num: 290,
    category: Category::Schema,
    severity: Severity::Error,
    slug: "validation",
};

impl NikaErrorCode for SchemaError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::YamlSyntax { .. } => SCHEMA_280,
            Self::UnknownVerb { .. } => SCHEMA_281,
            Self::MissingField { .. } => SCHEMA_282,
            Self::InvalidTaskId { .. } => SCHEMA_283,
            Self::DuplicateTask { .. } => SCHEMA_284,
            Self::Cycle { .. } => SCHEMA_285,
            Self::UnknownDependency { .. } => SCHEMA_286,
            Self::InvalidStructuredOutput { .. } => SCHEMA_287,
            Self::InvalidGuardrail { .. } => SCHEMA_288,
            Self::UnsupportedVersion { .. } => SCHEMA_289,
            Self::Validation { .. } => SCHEMA_290,
        }
    }

    fn is_transient(&self) -> bool {
        false // Schema errors are never transient — the YAML is wrong.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_syntax_error_display() {
        let err = SchemaError::YamlSyntax {
            message: "unexpected token".into(),
            span: None,
        };
        assert!(err.to_string().contains("YAML parse error"));
        assert!(err.to_string().contains("unexpected token"));
    }

    #[test]
    fn unknown_verb_display() {
        let err = SchemaError::UnknownVerb {
            verb: "dance".into(),
            span: None,
        };
        assert_eq!(err.to_string(), "unknown action verb: dance");
    }

    #[test]
    fn cycle_display() {
        let err = SchemaError::Cycle {
            cycle: vec!["a".into(), "b".into(), "c".into()],
        };
        assert_eq!(err.to_string(), "dependency cycle: a → b → c");
    }

    #[test]
    fn error_codes_are_in_schema_range() {
        let errors: Vec<SchemaError> = vec![
            SchemaError::YamlSyntax {
                message: String::new(),
                span: None,
            },
            SchemaError::UnknownVerb {
                verb: String::new(),
                span: None,
            },
            SchemaError::MissingField {
                field: String::new(),
                span: None,
            },
            SchemaError::InvalidTaskId {
                id: String::new(),
                reason: String::new(),
                span: None,
            },
            SchemaError::DuplicateTask {
                name: String::new(),
                span: None,
            },
            SchemaError::Cycle { cycle: vec![] },
            SchemaError::UnknownDependency {
                from: String::new(),
                to: String::new(),
                span: None,
            },
            SchemaError::InvalidStructuredOutput {
                reason: String::new(),
                span: None,
            },
            SchemaError::InvalidGuardrail {
                reason: String::new(),
                span: None,
            },
            SchemaError::UnsupportedVersion {
                version: String::new(),
            },
            SchemaError::Validation {
                message: String::new(),
                span: None,
            },
        ];

        for err in &errors {
            let code = err.nika_code();
            assert!(
                (280..=329).contains(&code.num),
                "SchemaError code {} should be in 280-329 range",
                code.num,
            );
            assert_eq!(code.category, Category::Schema);
        }
    }

    #[test]
    fn all_codes_unique() {
        let errors: Vec<SchemaError> = vec![
            SchemaError::YamlSyntax {
                message: String::new(),
                span: None,
            },
            SchemaError::UnknownVerb {
                verb: String::new(),
                span: None,
            },
            SchemaError::MissingField {
                field: String::new(),
                span: None,
            },
            SchemaError::InvalidTaskId {
                id: String::new(),
                reason: String::new(),
                span: None,
            },
            SchemaError::DuplicateTask {
                name: String::new(),
                span: None,
            },
            SchemaError::Cycle { cycle: vec![] },
            SchemaError::UnknownDependency {
                from: String::new(),
                to: String::new(),
                span: None,
            },
            SchemaError::InvalidStructuredOutput {
                reason: String::new(),
                span: None,
            },
            SchemaError::InvalidGuardrail {
                reason: String::new(),
                span: None,
            },
            SchemaError::UnsupportedVersion {
                version: String::new(),
            },
            SchemaError::Validation {
                message: String::new(),
                span: None,
            },
        ];

        let codes: Vec<u16> = errors.iter().map(|e| e.nika_code().num).collect();
        let mut deduped = codes.clone();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(codes.len(), deduped.len(), "all error codes must be unique");
    }

    #[test]
    fn schema_errors_are_never_transient() {
        let err = SchemaError::YamlSyntax {
            message: "bad".into(),
            span: None,
        };
        assert!(!err.is_transient());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn schema_error_is_send_sync() {
        _assert_send_sync::<SchemaError>();
    }
}
