// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-cel` — the `cel-subset/0.1` expression engine (L0).
//!
//! The ONE CEL parser + computer the whole engine shares — the spec's
//! "zero parser drift between engines" goal made structural. The
//! checker (nika-schema · static shape) and the runtime (nika-runtime ·
//! the `expr` seam) both consume it; neither re-implements the grammar.
//! Spec: `docs/crate-specs/nika-cel.md`.
//!
//! # Safety
//!
//! This crate computes the `cel-subset/0.1` grammar — **non-Turing-
//! complete, side-effect-free, bounded** (the CEL design · used by
//! Kubernetes/Envoy/gRPC). It is NOT a general code interpreter: no
//! I/O, no loops, no recursion, no host calls; the callable set is
//! closed (spec §3). "Compute" means "compute one bounded expression's
//! value", never "run arbitrary code".
//!
//! ```
//! use nika_cel::{parse, compute_bool, Resolver};
//! use serde_json::{json, Value};
//!
//! struct Vars;
//! impl Resolver for Vars {
//!     fn resolve_root(&self, name: &str) -> Option<Value> {
//!         (name == "vars").then(|| json!({ "publish": "yes" }))
//!     }
//! }
//!
//! let expr = parse("vars.publish == 'yes'").expect("parses");
//! assert!(compute_bool(&expr, &Vars).expect("computes"));
//! ```

#![forbid(unsafe_code)]

mod ast;
mod compute;
mod error;
mod lexer;
mod parser;

pub use ast::Expr;
pub use compute::{Resolver, compute, compute_bool};
pub use error::{CelError, CelErrorKind};
pub use parser::parse;

/// The grammar version this crate implements (spec 03-dag).
pub const GRAMMAR_VERSION: &str = "cel-subset/0.1";
