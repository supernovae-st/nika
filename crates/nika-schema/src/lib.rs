// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-schema` — workflow AST, parser, analyzer, and DAG validation.
//!
//! This crate sits at **L0**: pure, zero I/O, zero async.
//!
//! # Three-phase pipeline
//!
//! ```text
//! YAML string
//!     │
//!     ▼
//! [Parser]  ── YAML → RawWorkflow (spans, no validation)
//!     │
//!     ▼
//! [Analyzer] ── Raw → AnalyzedWorkflow (taint, guardrails, verb checks, DAG)
//!     │
//!     ▼
//! [Validator] ─ cycle detection, topological sort, schema validation
//!     │
//!     ▼
//! AnalyzedWorkflow (ready for lowering in nika-runtime)
//! ```

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
        clippy::float_cmp,
        clippy::manual_string_new,
        clippy::panic,
        clippy::unreachable,
    )
)]

pub mod analyzer;
pub mod check;
pub mod codegen;
pub mod error;
pub mod expression;
pub mod lints;
pub mod parser;
pub mod raw;
pub mod source;
mod suggest;
pub mod trust;
pub mod types;

// Re-exports for convenience.
pub use analyzer::{AnalyzedWorkflow, analyze};
pub use check::{
    ByteSpan, CapabilityEscape, CheckReport, ConformanceViolation, CostCeiling, Hint,
    InferredPermits, REPORT_VERSION, SchemaLintFinding, SchemaTypeFinding, SecretLeak, TaskCost,
    UnknownTool, check, infer_permits,
};
pub use codegen::nika_builtin_tool_enum_schema;
pub use error::{SchemaError, SpecCategory, SpecCode};
pub use parser::{ParseMode, parse};
pub use source::{ByteOffset, FileId, LineCol, SourceFile, SourceRegistry, Span, Spanned};

// Type re-exports (§3.16 of crate spec).
pub use types::{
    BackoffStrategy, CaptureMode, ExtractMode, GoDurationError, OnError, OutputDecl, ResponseMode,
    RetryConfig, SchemaVersion, SecretRef, SecretSource, VarDecl, VarType, is_valid_error_code,
    parse_go_duration,
};
