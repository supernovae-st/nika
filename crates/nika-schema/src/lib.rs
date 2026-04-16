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
    )
)]

pub mod error;
pub mod guardrails;
pub mod raw;
pub mod source;
pub mod trust;
pub mod types;

// Re-exports for convenience.
pub use error::SchemaError;
pub use source::{ByteOffset, FileId, LineCol, SourceFile, SourceRegistry, Span, Spanned};

// Type re-exports (§3.16 of crate spec).
pub use types::{
    AgentDef, ArtifactSpec, ArtifactsConfig, Budget, BudgetConfig, CompletionConfig,
    CompletionMode, ContentPart, ContextConfig, DecomposeSpec, DecomposeStrategy, ExtractMode,
    IncludeSpec, LimitAction, LimitStatus, LimitType, LimitsConfig, LogConfig, LogFormat, LogLevel,
    OrchestrateConfig, OutputFormat, OutputPolicy, RecordSpec, ResponseMode, RoutingConfig,
    ScheduleConfig, SchemaRef, SchemaVersion, StructuredOutputSpec, Templatable,
};
