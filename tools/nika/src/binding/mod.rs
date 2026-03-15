//! Binding Module - Data binding between tasks.
//!
//! Handles `with:` block data binding:
//! - `entry`: YAML types (BindingSpec/BindingEntry + WithSpec/WithEntry)
//! - `resolve`: Runtime resolution (ResolvedBindings) with lazy support
//! - `template`: Template substitution (`{{with.alias}}`)
//! - `jsonpath`: RFC 9535 JSONPath via serde_json_path
//! - `types`: Core types (BindingPath, BindingSource, PathSegment, BindingType)
//! - `transform`: 27 built-in transforms with pipe chains
//!
//! Unified `with:` syntax (eager resolution):
//! ```yaml
//! with:
//!   forecast: weather.summary           # Simple path
//!   temp: weather.data.temp ?? 20       # With numeric default
//!   name: user.name ?? "Anonymous"      # With string default (quoted)
//!   cfg: settings ?? {"debug": false}   # With object default
//! ```
//!
//! Extended syntax for lazy bindings:
//! ```yaml
//! with:
//!   lazy_val:
//!     path: future.result
//!     lazy: true                        # Deferred resolution
//!   lazy_with_default:
//!     path: optional.value
//!     lazy: true
//!     default: "fallback"
//! ```
//!
//! Data flow:
//! ```text
//! YAML `with:` block → BindingSpec (entry)
//!                          ↓
//!                  ┌───────┴───────┐
//!                  ▼               ▼
//!           Eager (lazy=false)  Lazy (lazy=true)
//!           resolve now         store Pending
//!                  │               │
//!                  ▼               ▼
//!           ResolvedBindings (Resolved | Pending)
//!                          ↓
//!                  Template substitution
//!                  (resolves Pending on access)
//!                          ↓
//!                    Resolved prompt
//! ```

mod entry;
pub mod jsonpath;
pub mod mention;
mod resolve;
mod template;
pub mod transform;
pub mod types;
mod validate;

// Re-export public types
pub use entry::{
    parse_binding_entry, parse_with_entry, BindingEntry, BindingSpec, WithEntry,
    WithEntryParseError, WithSpec,
};
pub use mention::{
    has_parallel_marker, mentions_to_bindings, parse_mentions, resolve_mention,
    strip_parallel_marker, text_to_bindings, Mention, MentionResolutionError, ResolvedMention,
};
pub use resolve::{LazyBinding, ResolvedBindings};
pub use template::{
    escape_for_shell, extract_refs, extract_with_refs, parse_template_expr,
    resolve as template_resolve, resolve_for_shell as template_resolve_for_shell,
    resolve_with as template_resolve_with, validate_refs, validate_with_refs, TemplateExpr,
};
pub use transform::{TransformError, TransformExpr, TransformOp, TransformParseError};
pub use types::{BindingPath, BindingPathError, BindingSource, BindingType, PathSegment};
pub use validate::validate_task_id;
