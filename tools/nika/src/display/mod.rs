//! CLI display — header, event stream, summary renderers.
//!
//! ## Module structure
//! - `legacy` — Original display functions (to be gradually replaced)
//! - `icons` — Cosmic icon palette (verb, status, subsystem)
//! - `colors` — Color constants and helpers

pub mod colors;
pub mod icons;
mod legacy;

// Re-export legacy API so nothing breaks
pub use legacy::*;
