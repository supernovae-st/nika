//! CLI display — header, event stream, summary renderers.
//!
//! ## Module structure
//! - `legacy` — Original display functions (to be gradually replaced)
//! - `icons` — Cosmic icon palette (verb, status, subsystem)
//! - `colors` — Color constants and helpers

pub mod colors;
pub mod dag;
pub mod detail;
pub mod header;
pub mod icons;
mod legacy;
pub mod renderer;

// Re-export legacy API so nothing breaks
pub use detail::DetailLevel;
pub use legacy::*;
pub use renderer::CliRenderer;
