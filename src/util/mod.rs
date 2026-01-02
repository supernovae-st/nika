//! Utilities Module - shared infrastructure (v0.1)
//!
//! Contains helper functions and data structures used across the codebase:
//! - `interner`: String interning for recurring task IDs (Arc<str> deduplication)
//! - `jsonpath`: Minimal JSONPath parser for path resolution

mod interner;
pub mod jsonpath;

// Re-export public types
pub use interner::{intern, Interner};
