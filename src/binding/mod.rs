//! Binding Module - Data binding between tasks (v0.1)
//!
//! Handles the `use:` block system for explicit data binding:
//! - `entry`: YAML types (UseWiring, UseEntry) - unified syntax
//! - `resolve`: Runtime resolution (UseBindings)
//! - `template`: Template substitution (`{{use.alias}}`)
//!
//! Unified `use:` syntax:
//! ```yaml
//! use:
//!   forecast: weather.summary           # Simple path
//!   temp: weather.data.temp ?? 20       # With numeric default
//!   name: user.name ?? "Anonymous"      # With string default (quoted)
//!   cfg: settings ?? {"debug": false}   # With object default
//! ```
//!
//! Data flow:
//! ```text
//! YAML `use:` block → UseWiring (entry)
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
mod validate;

// Re-export public types
pub use entry::{parse_use_entry, UseEntry, UseWiring};
pub use resolve::UseBindings;
pub use template::{extract_refs, resolve as template_resolve, validate_refs};
pub use validate::validate_task_id;
