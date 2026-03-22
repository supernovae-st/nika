//! Utilities Module - shared infrastructure
//!
//! Contains helper functions and data structures used across the codebase:
//! - `constants`: Centralized timeouts and limits
//! - `fs`: Atomic file write operations
//! - `interner`: String interning for recurring task IDs (`Arc<str>` deduplication)
//! - `system`: Platform-specific system information (RAM detection, etc.)

pub mod constants;
pub mod fs;
mod interner;
pub mod system;

// Re-export actively used types
pub use constants::{
    CONNECT_TIMEOUT, DECOMPOSE_TIMEOUT, EXEC_TIMEOUT, FETCH_TIMEOUT, INVOKE_TASK_DEADLINE,
    REDIRECT_LIMIT, STREAM_CHUNK_TIMEOUT,
};
// MCP_CALL_TIMEOUT and RECONNECT_TIMEOUT moved to nika-mcp crate
pub use fs::{atomic_write, check_preview_size, format_size};
pub use interner::intern;

/// Truncate a string at a valid UTF-8 char boundary.
///
/// Returns a slice of at most `max_bytes` bytes, ending at a char boundary.
/// Avoids panics from byte-slicing multi-byte UTF-8 sequences (CJK, emoji, etc.).
///
/// # Example
/// ```ignore
/// let s = "こんにちは世界"; // 21 bytes
/// assert_eq!(truncate_str(s, 10), "こんに"); // 9 bytes, safe boundary
/// ```
pub fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last char boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
