//! Nika — re-exports from nika-engine for backward compatibility.
//!
//! The execution engine lives in the `nika-engine` crate.
//! This re-export layer ensures existing `use nika::*` imports continue working.

pub use nika_engine::*;

// TUI crate re-export (feature-gated)
#[cfg(feature = "tui")]
pub use nika_tui as tui;
