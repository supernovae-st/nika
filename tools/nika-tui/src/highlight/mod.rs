// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Syntax Highlighting Module
//!
//! Provides tree-sitter based syntax highlighting for YAML workflows.
//! Inspired by Helix editor's approach - incremental parsing with semantic awareness.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         HIGHLIGHT PIPELINE                              │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │                                                                         │
//! │  Source Text ──► Tree-sitter Parser ──► Syntax Tree ──► Highlights     │
//! │                        │                     │              │          │
//! │                   tree-sitter-yaml      query.captures()   Styled Spans │
//! │                                                                         │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use nika_tui::highlight::{Highlighter, TreeSitterHighlighter};
//!
//! let highlighter = TreeSitterHighlighter::new()?;
//! let styled_lines = highlighter.highlight(yaml_source);
//! ```

mod theme;
mod treesitter;

pub use theme::{HighlightTheme, SolarizedTheme};
pub use treesitter::TreeSitterHighlighter;

use ratatui::text::Line;

/// Trait for syntax highlighters
///
/// Implementations parse source text and return styled lines for rendering.
pub trait Highlighter {
    /// Highlight source text and return styled lines
    ///
    /// # Arguments
    /// * `source` - The source text to highlight
    ///
    /// # Returns
    /// Vector of styled `Line` objects for ratatui rendering
    fn highlight<'a>(&self, source: &'a str) -> Vec<Line<'a>>;

    /// Incrementally update highlighting after an edit
    ///
    /// # Arguments
    /// * `source` - The updated source text
    /// * `start_byte` - Byte offset where edit started
    /// * `old_end_byte` - Original end byte of edited region
    /// * `new_end_byte` - New end byte after edit
    ///
    /// # Returns
    /// Vector of styled lines (may reuse cached tree)
    fn highlight_incremental<'a>(
        &mut self,
        source: &'a str,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
    ) -> Vec<Line<'a>>;
}

/// Highlight capture names used in tree-sitter queries
///
/// These correspond to @capture names in highlight.scm queries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightCapture {
    /// YAML keys (mapping keys)
    Key,
    /// String values (quoted or plain)
    String,
    /// Numeric values (int, float)
    Number,
    /// Boolean values (true, false)
    Boolean,
    /// Null value
    Null,
    /// Comments
    Comment,
    /// Punctuation (colons, brackets, etc.)
    Punctuation,
    /// Nika-specific: task verbs (infer, exec, fetch, invoke, agent)
    Verb,
    /// Nika-specific: task IDs
    TaskId,
    /// Nika-specific: template expressions ({{with.x}})
    Template,
    /// Nika-specific: MCP server names
    McpServer,
    /// Error nodes (parse errors)
    Error,
}

impl HighlightCapture {
    /// Map capture name from tree-sitter query to enum variant
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "keyword" | "key" | "property" => Some(Self::Key),
            "string" => Some(Self::String),
            "number" => Some(Self::Number),
            "constant.builtin" | "boolean" => Some(Self::Boolean),
            "constant" | "null" => Some(Self::Null),
            "comment" => Some(Self::Comment),
            "punctuation" | "punctuation.bracket" | "punctuation.delimiter" => {
                Some(Self::Punctuation)
            }
            "function" | "verb" => Some(Self::Verb),
            "label" | "task_id" => Some(Self::TaskId),
            "embedded" | "template" => Some(Self::Template),
            "namespace" | "mcp_server" => Some(Self::McpServer),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_from_name() {
        assert_eq!(
            HighlightCapture::from_name("keyword"),
            Some(HighlightCapture::Key)
        );
        assert_eq!(
            HighlightCapture::from_name("string"),
            Some(HighlightCapture::String)
        );
        assert_eq!(
            HighlightCapture::from_name("number"),
            Some(HighlightCapture::Number)
        );
        assert_eq!(
            HighlightCapture::from_name("constant.builtin"),
            Some(HighlightCapture::Boolean)
        );
        assert_eq!(
            HighlightCapture::from_name("comment"),
            Some(HighlightCapture::Comment)
        );
        assert_eq!(
            HighlightCapture::from_name("function"),
            Some(HighlightCapture::Verb)
        );
        assert_eq!(HighlightCapture::from_name("unknown"), None);
    }
}
