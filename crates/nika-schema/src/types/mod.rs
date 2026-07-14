// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow configuration types — the vocabulary of `.nika.yaml` files.
//!
//! These types represent the configuration options available in workflow
//! definitions (per the canonical `nika-spec` v1 language · `spec/01..05`).
//! They are used by both the raw AST (parser output) and the analyzed AST
//! (post-validation).

pub mod after;
pub mod capture;
pub mod duration;
pub mod extract;
pub mod on_error;
pub mod output_decl;
pub mod permits;
pub mod retry;
pub mod schema_version;
pub mod secret;
pub mod var_decl;
pub mod when_gate;

// Re-exports for convenience.
pub use after::AfterPredicate;
pub use capture::CaptureMode;
pub use duration::{GoDurationError, parse_go_duration};
pub use extract::{ExtractMode, ResponseMode};
pub use on_error::{OnError, OnErrorAction};
pub use output_decl::OutputDecl;
pub use permits::{ExecPermit, FsPermits, NetPermits, Permits};
pub use retry::{BackoffStrategy, RetryConfig, is_valid_error_code};
pub use schema_version::SchemaVersion;
pub use secret::{EgressRule, SecretRef, SecretSource};
pub use var_decl::{VarDecl, VarType};
pub use when_gate::WhenGate;
