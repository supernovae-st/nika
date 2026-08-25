#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's dataflow — descended from `nika-runtime` 2026-08-25 at the 15k
//! prod-LOC wall (the `nika-secret`/`nika-proof`/`nika-dap`/`nika-cap`
//! precedent). Two questions live here, and they are one question:
//!
//! 1. **What a task record IS** ([`record`]) — status · output · error ·
//!    timing (spec 04 §task reference).
//! 2. **How a value referencing those records resolves** ([`expr`] · [`jq`])
//!    — `${{ }}` islands, `cel-subset/0.1` gate expressions, and `output:`
//!    named jq bindings.
//!
//! They descend together because they are not separable: [`expr`] projects a
//! [`record::TaskRecord`] into the CEL object a `${{ tasks.x.output }}`
//! island reads, and renders values back out through
//! [`record::render_value`]. A seam between them would cut a single concept
//! in half.
//!
//! ## Why this is a crate and not a module
//!
//! The whole surface is PURE — zero I/O, zero async, zero clock. Given a
//! scope (the records + the value authorities) and a value, it answers with
//! a value or a [`DataflowError`]. That is a deep module: a large
//! implementation (CEL evaluation · quote-aware island lexing · jq program
//! compilation · the closed reserved-field projection) behind a handful of
//! entry points. The executor keeps the effects; this keeps the evaluation.
//!
//! ## The seam did not move
//!
//! `nika-runtime` re-exports [`record::TaskRecord`], [`record::TaskStatus`],
//! [`record::TerminalCause`], [`record::TaskErrorRecord`] and
//! [`record::legal`] at their historical paths. Today's four
//! [`DataflowError`] classes convert back into their four historical
//! `RuntimeError` constructors; `RuntimeError::Dataflow` is the
//! forward-compatible fallback for future classes. The wire form
//! (`NIKA-VAR-001` · `-002` · `-004` · `-005` · `-006`) a consumer sees is
//! byte-identical to before the descent.

mod errors;

pub mod expr;
pub mod jq;
pub mod record;

pub use errors::DataflowError;
pub use expr::Scope;
pub use record::{TaskErrorRecord, TaskRecord, TaskStatus, TerminalCause, legal};
