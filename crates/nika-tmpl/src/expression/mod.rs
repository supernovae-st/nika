// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `${{ … }}` expression layer — CEL v0.1 subset.
//!
//! Spec `03-dag.md` §CEL-subset · « Everything inside `${{ ... }}` …
//! is **CEL** (Common Expression Language) … Nika does **not** invent
//! an expression DSL — it adopts the standard. » The normative grammar
//! is `cel-subset/0.1` (a strict subset of cel-spec · « hand-roll the
//! small v0.1 subset … conformant because the subset is exactly CEL »).
//!
//! Surfaces ·
//!
//! - [`parse_expression`] — one expression (the inside of an island).
//! - [`scan_templates`] — find + parse all `${{ … }}` islands in a
//!   string (honors the `\${{` literal escape).
//! - [`expr_refs`] — classify root references against the 5 namespaces
//!   + 2 `for_each` loop-locals (feeds `NIKA-VAR-001` / `NIKA-VAR-021`).
//! - [`task_output_paths`] — every `tasks.<id>.output.<path…>` chain
//!   with its FULL path (the dataflow schema-typing surface · ADR-092 #4).
//! - [`is_boolean_shaped`] — the static `when:` shape gate
//!   (`NIKA-PARSE-WHEN-001`).
//!
//! Pure L0 · zero deps · zero I/O · the parser never panics (proptest-
//! locked).

mod ast;
mod error;
mod lexer;
mod parser;
mod refs;
mod template;

pub use ast::{Expr, Literal, NamespaceRef, RelOp};
pub use error::ExprError;
pub use parser::parse_expression;
pub use refs::{bare_task_refs, expr_refs, is_boolean_shaped, task_output_paths};
pub use template::{TemplateIsland, scan_templates};
