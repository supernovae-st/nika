//! Binding Module - Data binding between tasks (v0.5)
//!
//! Handles the `use:` block system for explicit data binding:
//! - `entry`: YAML types (WiringSpec, UseEntry) - unified and extended syntax
//! - `resolve`: Runtime resolution (ResolvedBindings) with lazy support
//! - `template`: Template substitution (`{{use.alias}}`)
//!
//! Unified `use:` syntax (eager resolution):
//! ```yaml
//! use:
//!   forecast: weather.summary           # Simple path
//!   temp: weather.data.temp ?? 20       # With numeric default
//!   name: user.name ?? "Anonymous"      # With string default (quoted)
//!   cfg: settings ?? {"debug": false}   # With object default
//! ```
//!
//! Extended syntax for lazy bindings (v0.5 MVP 8):
//! ```yaml
//! use:
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
//! YAML `use:` block → WiringSpec (entry)
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
mod resolve;
mod template;
mod validate;

// Re-export public types
pub use entry::{parse_use_entry, UseEntry, WiringSpec};
pub use resolve::{LazyBinding, ResolvedBindings};
pub use template::{extract_refs, resolve as template_resolve, validate_refs};
pub use validate::validate_task_id;
