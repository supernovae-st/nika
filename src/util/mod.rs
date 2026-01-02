//! Utilities Module - shared infrastructure (v0.1)
//!
//! Contains helper functions and data structures used across the codebase:
//! - `constants`: Centralized timeouts and limits
//! - `interner`: String interning for recurring task IDs (Arc<str> deduplication)
//! - `jsonpath`: Minimal JSONPath parser for path resolution

pub mod constants;
mod interner;
pub mod jsonpath;

// Re-export public types
pub use constants::{CONNECT_TIMEOUT, EXEC_TIMEOUT, FETCH_TIMEOUT, INFER_TIMEOUT, REDIRECT_LIMIT};
pub use interner::{intern, Interner};
