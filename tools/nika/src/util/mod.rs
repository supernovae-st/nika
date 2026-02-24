//! Utilities Module - shared infrastructure (v0.1)
//!
//! Contains helper functions and data structures used across the codebase:
//! - `constants`: Centralized timeouts and limits
//! - `fs`: Atomic file write operations
//! - `interner`: String interning for recurring task IDs (Arc<str> deduplication)
//! - `jsonpath`: Minimal JSONPath parser for path resolution

pub mod constants;
pub mod fs;
mod interner;
pub mod jsonpath;

// Re-export public types
pub use constants::{
    CONNECT_TIMEOUT, EXEC_TIMEOUT, FETCH_TIMEOUT, INFER_TIMEOUT, MCP_CALL_TIMEOUT, MCP_INIT_TIMEOUT,
    REDIRECT_LIMIT, STREAM_CHUNK_TIMEOUT,
};
pub use fs::{atomic_write, atomic_write_async, check_preview_size, format_size};
pub use interner::{intern, Interner};
