//! Binding Module - Data binding between tasks (v0.1)
//!
//! Handles the `use:` block system for explicit data binding:
//! - `spec`: YAML types (UseWiring, UseEntry, UseAdvanced)
//! - `resolve`: Runtime resolution (UseBindings)
//! - `template`: Template substitution (`{{use.alias}}`)
//!
//! Data flow:
//! ```text
//! YAML `use:` block → UseWiring (spec)
//!                          ↓
//!                  Runtime resolution
//!                          ↓
//!                    UseBindings (resolve)
//!                          ↓
//!                  Template substitution
//!                          ↓
//!                    Resolved prompt
//! ```

mod entry;
mod resolve;
mod template;

// Re-export public types
pub use entry::{UseAdvanced, UseEntry, UseWiring};
pub use resolve::UseBindings;
pub use template::{extract_refs, resolve as template_resolve, validate_refs};
