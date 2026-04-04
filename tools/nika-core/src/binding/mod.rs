//! Core Binding Types
//!
//! Pure type definitions for the binding system, with no runtime dependencies.
//!
//! - `types`: BindingPath, BindingSource, PathSegment, BindingType
//! - `entry`: BindingSpec, BindingEntry, WithSpec, WithEntry, parse_with_entry
//! - `transform`: TransformOp, TransformExpr, 27 built-in transforms
//!
//! Runtime-dependent modules (resolve, template, jsonpath, validate, mention)
//! remain in the `nika` crate.

mod entry;
pub mod transform;
pub mod types;

// Re-export public types
pub use entry::{
    parse_binding_entry, parse_with_entry, BindingEntry, BindingSpec, WithEntry,
    WithEntryParseError, WithSpec,
};
pub use transform::{
    deep_merge, eval_jq, navigate_dot_path, TransformError, TransformExpr, TransformOp,
    TransformParseError,
};
pub use types::{BindingPath, BindingPathError, BindingSource, BindingType, PathSegment};
