// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow configuration types — the vocabulary of `.nika.yaml` files.
//!
//! These types represent the configuration options available in workflow
//! definitions (per the canonical `nika-spec` v1 language · `spec/01..05`).
//! They are used by both the raw AST (parser output) and the analyzed AST
//! (post-validation).
//!
//! The sibling [`project`] module is the ONE exception to the workflow
//! scope: the project file `nika.yaml` (D-2026-08-11-N5) — project-scope
//! config vocabulary, NOT a workflow. It lives here rather than in the
//! schema unit so every consumer (the L2 registry gate · the L4 CLI)
//! depends DOWN on it.
//!
//! Split out of `nika-schema` per the size-cap discipline
//! (D-2026-07-09-N1 · one architectural unit, N workspace members · the
//! `nika-source` precedent): `nika_schema::types` re-exports this crate
//! wholesale, so every consumer path (`nika_schema::types::Permits` ·
//! `VarDecl` · `RetryConfig` · …) is unchanged — the schema crate remains
//! the unit's front door.

#![forbid(unsafe_code)]
#![warn(
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    missing_docs
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
    )
)]

pub mod after;
pub mod capture;
pub mod dead_form;
pub mod decode;
pub mod duration;
pub mod extract;
pub mod keys;
pub mod on_error;
pub mod output_decl;
pub mod permits;
pub mod project;
pub mod registry_ref;
pub mod retry;
pub mod run;
pub mod schema_version;
pub mod secret;
pub mod tool_ref;
pub mod type_expr;
pub mod var_decl;
pub mod when_gate;

// Re-exports for convenience.
pub use after::AfterPredicate;
pub use capture::CaptureMode;
pub use dead_form::DeadForm;
pub use decode::DecodeMode;
pub use duration::{GoDurationError, parse_go_duration};
pub use extract::{ExtractMode, ResponseMode};
pub use on_error::{OnError, OnErrorAction};
pub use output_decl::OutputDecl;
// The effect vocabulary (spec 10 · W4) — home is `nika-cap`. `Policy`,
// `Objective` and `PolicyViolation` left with the `policy:` block.
pub use nika_cap::EffectClass;
pub use permits::{ExecPermit, FsPermits, NetPermits, Permits};
pub use retry::{BackoffStrategy, RetryConfig, is_valid_error_code};
pub use run::{RunClock, RunContradiction, RunDecl, RunEntropy};
pub use schema_version::SchemaVersion;
pub use secret::{EgressRule, SecretRef, SecretSource};
pub use type_expr::{coerce_declared, default_not_conforming_teaching, type_expr_display};
pub use var_decl::VarDecl;
pub use when_gate::WhenGate;
