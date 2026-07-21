// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-schema` — the workflow AST and parser (THE PARSER · its blueprint
//! shape).
//!
//! This crate sits at **L0**: pure, zero I/O, zero async.
//!
//! # The one phase
//!
//! ```text
//! YAML string
//!     │
//!     ▼
//! [Parser]  ── YAML → RawWorkflow (spans, no validation)
//! ```
//!
//! The static judgment over the parsed AST — the analyzer (Core
//! conformance · derived DAG edges) and the `nika check` ladder — lives in
//! **`nika-check`** (split 2026-07-21 at the 15k crate-size wall · the
//! nika-graph/nika-dap precedents · L0, depends on THIS crate — never the
//! reverse).

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

pub mod error;
/// The `${{ }}` expression language — DESCENDED to `nika-tmpl` (2026-07-10 ·
/// the 15k crate-size wall · the trace→dap precedent). Re-exported verbatim:
/// every `nika_schema::expression::…` path resolves unchanged.
pub use nika_tmpl::expression;
pub mod parser;
pub mod raw;
pub mod skill;
pub mod source;
pub mod types;

// Re-exports for convenience.
pub use error::{SchemaError, SpecCategory, SpecCode};
pub use nika_catalog::codegen::nika_builtin_tool_enum_schema;
pub use parser::{ParseMode, parse};
pub use skill::{
    ResolvedSkills, SkillDefect, SkillDoc, SkillFinding, parse_skill, resolve_skills, skill_refs,
};
pub use source::{ByteOffset, FileId, LineCol, SourceFile, SourceRegistry, Span, Spanned};

// Type re-exports (§3.16 of crate spec).
pub use types::{
    BackoffStrategy, CaptureMode, DecodeMode, ExtractMode, GoDurationError, OnError, OutputDecl,
    ResponseMode, RetryConfig, SchemaVersion, SecretRef, SecretSource, VarDecl,
    is_valid_error_code, parse_go_duration,
};
