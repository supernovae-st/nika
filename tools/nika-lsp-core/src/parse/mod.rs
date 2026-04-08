// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error-recovery parsing layer for the LSP.
//!
//! Uses tree-sitter-yaml to extract structural information from
//! potentially broken YAML. This is the **structure-only** bridge:
//! tree-sitter gives us key positions and block structure, but values
//! are never trusted (YAML 1.1 vs 1.2 differences).
//!
//! # Architecture
//!
//! ```text
//! broken YAML ──> RecoveryParser ──> tree_sitter::Tree
//!                                        │
//!                                        ▼
//!                                   extract_partial()
//!                                        │
//!                                        ▼
//!                                  PartialWorkflow
//! ```

pub mod bridge;
pub mod recovery;

// ---------------------------------------------------------------------------
// TextRange
// ---------------------------------------------------------------------------

/// Byte-offset range in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

impl TextRange {
    /// Create a new range from start/end byte offsets.
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Length of this range in bytes.
    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Whether this range is empty.
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Whether this range contains a byte offset.
    pub fn contains(&self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }
}

// ---------------------------------------------------------------------------
// PartialField
// ---------------------------------------------------------------------------

/// A key-value field extracted from the CST.
///
/// `value` is the raw text from the source; it is NOT interpreted.
/// For actual YAML value semantics, use `marked_yaml`.
#[derive(Debug, Clone)]
pub struct PartialField {
    /// Raw text of the value (not YAML-interpreted).
    pub value: String,
    /// Byte range of the key in source.
    pub key_span: TextRange,
    /// Byte range of the value in source (None if key-only).
    pub value_span: Option<TextRange>,
}

// ---------------------------------------------------------------------------
// PartialTask
// ---------------------------------------------------------------------------

/// Structural info about a single task extracted from the CST.
///
/// This captures enough structure for completions and diagnostics
/// without trusting tree-sitter's value interpretation.
#[derive(Debug, Clone)]
pub struct PartialTask {
    /// Task ID (raw text, may be empty for incomplete tasks).
    pub id: Option<String>,
    /// Verb key: "infer", "exec", "fetch", "invoke", or "agent".
    pub verb: Option<String>,
    /// All top-level keys found in this task block.
    pub existing_keys: Vec<String>,
    /// Whether the task has a `content:` key.
    pub has_content: bool,
    /// Whether the task has a `with:` key.
    pub has_with: bool,
    /// Whether the task has a `depends_on:` key.
    pub has_depends_on: bool,
    /// Alias names from `with:` bindings.
    pub with_aliases: Vec<String>,
    /// Task IDs referenced in `depends_on:`.
    pub depends_on_refs: Vec<String>,
    /// Byte range of the entire task block.
    pub span: TextRange,
    /// Byte range of the verb key (for diagnostics).
    pub verb_span: Option<TextRange>,
}

// ---------------------------------------------------------------------------
// PartialWorkflow
// ---------------------------------------------------------------------------

/// Structural info extracted from (possibly broken) YAML via tree-sitter.
///
/// This is the output of `extract_partial()`. It captures enough structure
/// for the LSP to provide completions, diagnostics, and hover info even
/// when the document has syntax errors.
#[derive(Debug, Clone, Default)]
pub struct PartialWorkflow {
    /// `schema:` field if present.
    pub schema: Option<PartialField>,
    /// `workflow:` field if present.
    pub workflow_name: Option<PartialField>,
    /// Tasks extracted from `tasks:` block.
    pub tasks: Vec<PartialTask>,
    /// MCP server names found under `mcp:`.
    pub mcp_servers: Vec<PartialField>,
    /// Context file entries found under `context:`.
    pub context_files: Vec<PartialField>,
    /// Include specs found under `include:`.
    pub includes: Vec<PartialField>,
    /// Input parameter names found under `inputs:`.
    pub input_names: Vec<PartialField>,
    /// Whether the tree has any errors.
    pub has_errors: bool,
    /// Byte ranges of all ERROR nodes in the CST.
    pub error_ranges: Vec<TextRange>,
    /// All top-level keys found in the document.
    pub top_level_keys: Vec<PartialField>,
}

/// Parse text with error recovery and extract structural information.
///
/// This is the main entry point for the error-recovery pipeline.
/// Always returns a result, even for broken YAML — tree-sitter never fails.
///
/// # Example
/// ```ignore
/// let partial = parse_and_extract("schema: nika/workflow@0.12\ntasks:\n  - id: step1\n");
/// assert!(partial.schema.is_some());
/// assert_eq!(partial.tasks.len(), 1);
/// ```
pub fn parse_and_extract(text: &str) -> PartialWorkflow {
    let mut parser = recovery::RecoveryParser::new();
    match parser.parse(text) {
        Some(tree) => bridge::extract_partial(text, &tree),
        None => PartialWorkflow::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // TextRange tests
    // -----------------------------------------------------------------------

    #[test]
    fn text_range_new() {
        let r = TextRange::new(10, 20);
        assert_eq!(r.start, 10);
        assert_eq!(r.end, 20);
    }

    #[test]
    fn text_range_len() {
        assert_eq!(TextRange::new(0, 10).len(), 10);
        assert_eq!(TextRange::new(5, 5).len(), 0);
        assert_eq!(TextRange::default().len(), 0);
    }

    #[test]
    fn text_range_is_empty() {
        assert!(TextRange::new(5, 5).is_empty());
        assert!(TextRange::default().is_empty());
        assert!(!TextRange::new(0, 1).is_empty());
    }

    #[test]
    fn text_range_contains() {
        let r = TextRange::new(10, 20);
        assert!(!r.contains(9));
        assert!(r.contains(10));
        assert!(r.contains(15));
        assert!(r.contains(19));
        assert!(!r.contains(20));
    }

    #[test]
    fn text_range_default() {
        let r = TextRange::default();
        assert_eq!(r.start, 0);
        assert_eq!(r.end, 0);
        assert!(r.is_empty());
    }

    #[test]
    fn text_range_saturating_len() {
        // Inverted range should not panic
        let r = TextRange { start: 20, end: 10 };
        assert_eq!(r.len(), 0);
    }

    // -----------------------------------------------------------------------
    // PartialWorkflow default
    // -----------------------------------------------------------------------

    #[test]
    fn partial_workflow_default_is_empty() {
        let pw = PartialWorkflow::default();
        assert!(pw.schema.is_none());
        assert!(pw.workflow_name.is_none());
        assert!(pw.tasks.is_empty());
        assert!(pw.mcp_servers.is_empty());
        assert!(pw.context_files.is_empty());
        assert!(pw.includes.is_empty());
        assert!(pw.input_names.is_empty());
        assert!(!pw.has_errors);
        assert!(pw.error_ranges.is_empty());
        assert!(pw.top_level_keys.is_empty());
    }
}
