// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Utilities Module - shared infrastructure
//!
//! Contains helper functions and data structures used across the codebase:
//! - `constants`: Centralized timeouts and limits
//! - `fs`: Atomic file write operations
//! - `interner`: String interning for recurring task IDs (`Arc<str>` deduplication)
//! - `system`: Platform-specific system information (RAM detection, etc.)
//! - `parse_yaml_budgeted`: Hardened YAML deserialization with size/alias budgets

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
// Secret redaction moved to nika-core::util::redact
pub use nika_core::util::{redact_secrets, register_secrets};

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

/// Deserialize YAML with hardened budget limits.
///
/// Defends against YAML bombs (alias expansion, deeply nested structures,
/// oversized input) by enforcing:
/// - 1 MiB max input size
/// - 100 max anchors, 1000 max aliases
/// - 64 max depth
/// - alias/anchor ratio enforcement
///
/// Use this for ALL file-loaded YAML (workflows, skills, configs, partials).
/// Test code can continue using `serde_yaml::from_str` without budgets.
pub fn parse_yaml_budgeted<'de, T: serde::de::Deserialize<'de>>(
    content: &'de str,
) -> Result<T, crate::serde_yaml::Error> {
    use crate::serde_yaml::Budget;
    let options = crate::serde_yaml::Options {
        budget: Some(Budget {
            max_reader_input_bytes: Some(1_048_576), // 1 MiB
            max_aliases: 1_000,
            max_anchors: 100,
            max_depth: 64,
            enforce_alias_anchor_ratio: true,
            ..Budget::default()
        }),
        ..Default::default()
    };
    crate::serde_yaml::from_str_with_options(content, options)
}

