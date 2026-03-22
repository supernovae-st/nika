//! Nika — re-exports from nika-engine for backward compatibility.
//!
//! The execution engine lives in the `nika-engine` crate.
//! This re-export layer ensures existing `use nika::*` imports continue working.

pub use nika_engine::*;

// Feature-gated modules that stay in the nika binary crate
#[cfg(feature = "tui")]
pub mod tui;
