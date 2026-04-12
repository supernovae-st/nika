// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Core Binding Types
//!
//! Pure type definitions for the binding system, with no runtime dependencies.
//!
//! - `types`: BindingPath, BindingSource, PathSegment, BindingType
//! - `entry`: BindingSpec, BindingEntry, WithSpec, WithEntry, parse_with_entry
//! - `transform`: TransformOp, TransformExpr, 27 built-in transforms
//!
//! Runtime-dependent modules (resolve, template) remain in nika-engine.
//! Pure modules (mention, jsonpath, token_budget, validate) live here.

mod entry;
mod error;
pub mod jsonpath;
pub mod mention;
pub mod store;
pub mod token_budget;
pub mod transform;
pub mod types;
mod validate;

pub use error::BindingResolveError;
pub use store::{BindingStore, BindingStoreError};
pub use validate::validate_task_id;

// Re-export public types
pub use entry::{
    parse_binding_entry, parse_with_entry, BindingEntry, BindingSpec, WithEntry,
    WithEntryParseError, WithSpec,
};
pub use transform::{
    deep_merge, eval_jq, navigate_dot_path, TransformError, TransformExpr, TransformOp,
    TransformParseError,
};
pub use mention::{
    has_parallel_marker, mentions_to_bindings, parse_mentions, resolve_mention,
    strip_parallel_marker, text_to_bindings, Mention, MentionResolutionError, ResolvedMention,
};
pub use types::{BindingPath, BindingPathError, BindingSource, BindingType, PathSegment};
