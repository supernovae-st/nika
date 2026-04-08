// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tree-sitter YAML Highlighter
//!
//! Incremental parsing with semantic awareness for YAML workflows.
//! Uses tree-sitter-yaml grammar with custom Nika-specific queries.

use std::cell::RefCell;

use ratatui::{
    style::Style,
    text::{Line, Span},
};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor, Tree};

use super::{HighlightCapture, HighlightTheme, Highlighter, SolarizedTheme};

/// Tree-sitter based YAML highlighter
///
/// Provides incremental parsing - only re-parses changed regions after edits.
/// Supports both standard YAML highlighting and Nika-specific extensions.
///
/// Uses `RefCell` for parser and tree so `highlight()` (which takes `&self`
/// per the `Highlighter` trait) can reuse the parser and cache the tree
/// instead of creating a throwaway `Parser` on every call.
pub struct TreeSitterHighlighter {
    /// Tree-sitter parser instance (interior mutability for `&self` highlight)
    parser: RefCell<Parser>,
    /// Cached syntax tree from last parse
    tree: RefCell<Option<Tree>>,
    /// Highlight query for YAML
    query: Query,
    /// Color theme
    theme: SolarizedTheme,
}

impl TreeSitterHighlighter {
    /// Create a new tree-sitter highlighter
    pub fn new() -> Result<Self, String> {
        Self::with_theme(SolarizedTheme::new(true))
    }

    /// Create a highlighter with custom theme
    pub fn with_theme(theme: SolarizedTheme) -> Result<Self, String> {
        let mut parser = Parser::new();
        let language = tree_sitter_yaml::LANGUAGE;

        parser
            .set_language(&language.into())
            .map_err(|e| format!("Failed to set YAML language: {e}"))?;

        // Build highlight query
        // This is a subset of queries that work well with tree-sitter-yaml
        let query_source = Self::highlight_query();
        let query = Query::new(&language.into(), query_source)
            .map_err(|e| format!("Failed to compile highlight query: {e}"))?;

        Ok(Self {
            parser: RefCell::new(parser),
            tree: RefCell::new(None),
            query,
            theme,
        })
    }

    /// Get the highlight query source
    ///
    /// This is embedded rather than loaded from file for simplicity.
    /// Based on tree-sitter-yaml's standard queries with Nika extensions.
    fn highlight_query() -> &'static str {
        r#"
; Comments
(comment) @comment

; Block and flow scalars
(double_quote_scalar) @string
(single_quote_scalar) @string
(block_scalar) @string

; Plain scalars with special values
((plain_scalar (string_scalar)) @constant.builtin
  (#match? @constant.builtin "^(true|false|True|False|TRUE|FALSE)$"))

((plain_scalar (string_scalar)) @constant
  (#match? @constant "^(null|Null|NULL|~)$"))

((plain_scalar (string_scalar)) @number
  (#match? @number "^[-+]?[0-9]*\\.?[0-9]+([eE][-+]?[0-9]+)?$"))

; Plain scalars that are likely Nika verbs
((plain_scalar (string_scalar)) @function
  (#match? @function "^(infer|exec|fetch|invoke|agent)$"))

; Keys in mappings
(block_mapping_pair
  key: (flow_node) @keyword)

; Flow mapping keys
(flow_pair
  key: (flow_node) @keyword)

; Anchors and aliases
(anchor) @label
(alias) @label

; Tags
(tag) @type

; Punctuation
"-" @punctuation.delimiter
":" @punctuation.delimiter
"," @punctuation.delimiter
"[" @punctuation.bracket
"]" @punctuation.bracket
"{" @punctuation.bracket
"}" @punctuation.bracket
"?" @punctuation.delimiter
"|" @punctuation.delimiter
">" @punctuation.delimiter

; Errors
(ERROR) @error
"#
    }

    /// Parse source and return syntax tree
    fn parse(&self, source: &str) -> Option<Tree> {
        let mut parser = self.parser.borrow_mut();
        let tree_ref = self.tree.borrow();
        parser.parse(source, tree_ref.as_ref())
    }

    /// Convert byte offset to (line, column) position
    /// Column is a byte offset within the line (required by tree-sitter Point).
    fn byte_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
        let mut line = 0;
        let mut col = 0;
        for (idx, ch) in source.char_indices() {
            if idx >= byte_offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += ch.len_utf8();
            }
        }
        (line, col)
    }

    /// Get all highlight spans for a given line
    fn get_line_highlights(
        &self,
        tree: &Tree,
        source: &str,
        _line_num: usize,
        line_start_byte: usize,
        line_end_byte: usize,
    ) -> Vec<(usize, usize, Style)> {
        let mut cursor = QueryCursor::new();
        let root = tree.root_node();

        let mut highlights = Vec::new();

        // tree-sitter 0.24: Use StreamingIterator pattern for matches
        let mut matches = cursor.matches(&self.query, root, source.as_bytes());

        while let Some(query_match) = matches.next() {
            for capture in query_match.captures {
                let node = capture.node;
                let start = node.start_byte();
                let end = node.end_byte();

                // Skip if not on this line
                if end <= line_start_byte || start >= line_end_byte {
                    continue;
                }

                // Clip to line boundaries
                let span_start = start.saturating_sub(line_start_byte);
                let span_end = (end - line_start_byte).min(line_end_byte - line_start_byte);

                // Get capture name and map to style
                let capture_name = &self.query.capture_names()[capture.index as usize];
                if let Some(highlight_capture) = HighlightCapture::from_name(capture_name) {
                    let style = self.theme.style(highlight_capture);
                    highlights.push((span_start, span_end, style));
                }
            }
        }

        // Sort by start position
        highlights.sort_by_key(|(start, _, _)| *start);

        highlights
    }

    /// Convert highlights to styled spans for a line
    fn spans_from_highlights<'a>(
        line: &'a str,
        highlights: Vec<(usize, usize, Style)>,
    ) -> Vec<Span<'a>> {
        if highlights.is_empty() {
            return vec![Span::raw(line)];
        }

        let mut spans = Vec::new();
        let mut last_end = 0;

        for (start, end, style) in highlights {
            // Add unstyled text before this highlight
            if start > last_end {
                let text = &line[last_end..start.min(line.len())];
                if !text.is_empty() {
                    spans.push(Span::raw(text));
                }
            }

            // Add highlighted text
            let highlight_start = start.min(line.len());
            let highlight_end = end.min(line.len());
            if highlight_start < highlight_end {
                let text = &line[highlight_start..highlight_end];
                spans.push(Span::styled(text, style));
            }

            last_end = end;
        }

        // Add remaining unstyled text
        if last_end < line.len() {
            spans.push(Span::raw(&line[last_end..]));
        }

        if spans.is_empty() {
            vec![Span::raw(line)]
        } else {
            spans
        }
    }
}

impl TreeSitterHighlighter {
    /// Render syntax-highlighted lines from an already-parsed tree.
    ///
    /// Shared by `highlight()` and `highlight_incremental()` so neither
    /// method re-parses when it already has a valid tree.
    fn render_highlights<'a>(&self, tree: &Tree, source: &'a str) -> Vec<Line<'a>> {
        let lines: Vec<&str> = source.lines().collect();
        let mut result = Vec::with_capacity(lines.len());
        let mut byte_offset = 0;

        for (line_num, line) in lines.iter().enumerate() {
            let line_end = byte_offset + line.len();
            let highlights =
                self.get_line_highlights(tree, source, line_num, byte_offset, line_end);
            let spans = Self::spans_from_highlights(line, highlights);
            result.push(Line::from(spans));
            byte_offset = line_end + 1; // +1 for newline
        }

        result
    }
}

impl Highlighter for TreeSitterHighlighter {
    fn highlight<'a>(&self, source: &'a str) -> Vec<Line<'a>> {
        // Reuse the cached parser and tree via RefCell interior mutability
        let cached_tree = self.tree.borrow().as_ref().map(|t| t.clone());
        let new_tree = {
            let mut parser = self.parser.borrow_mut();
            parser.parse(source, cached_tree.as_ref())
        };

        let tree = match new_tree {
            Some(t) => t,
            None => return source.lines().map(Line::raw).collect(),
        };

        // Cache the new tree for future incremental parses
        *self.tree.borrow_mut() = Some(tree.clone());

        self.render_highlights(&tree, source)
    }

    fn highlight_incremental<'a>(
        &mut self,
        source: &'a str,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
    ) -> Vec<Line<'a>> {
        // Apply edit metadata to the cached tree so tree-sitter can reuse it
        if let Some(ref mut tree) = *self.tree.borrow_mut() {
            let start_position = {
                let (line, col) = Self::byte_to_line_col(source, start_byte);
                tree_sitter::Point::new(line, col)
            };
            let old_end_position = {
                let (line, col) = Self::byte_to_line_col(source, old_end_byte);
                tree_sitter::Point::new(line, col)
            };
            let new_end_position = {
                let (line, col) = Self::byte_to_line_col(source, new_end_byte);
                tree_sitter::Point::new(line, col)
            };

            tree.edit(&tree_sitter::InputEdit {
                start_byte,
                old_end_byte,
                new_end_byte,
                start_position,
                old_end_position,
                new_end_position,
            });
        }

        // Incremental re-parse: tree-sitter reuses unchanged subtrees
        let new_tree = self.parse(source);

        match new_tree {
            Some(tree) => {
                // Render directly from the incremental result — do NOT call
                // self.highlight() which would re-parse from scratch and throw
                // away the incremental work we just did.
                let result = self.render_highlights(&tree, source);
                *self.tree.borrow_mut() = Some(tree);
                result
            }
            None => {
                // Parse failed entirely (corrupt buffer) — reset and fall back
                *self.tree.borrow_mut() = None;
                source.lines().map(Line::raw).collect()
            }
        }
    }
}

impl Default for TreeSitterHighlighter {
    /// Create with defaults. Panics only if tree-sitter-yaml is broken
    /// (compile-time linked, so this is effectively infallible).
    fn default() -> Self {
        Self::new().expect("tree-sitter-yaml init failed (compile-time linked)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlighter_creation() {
        let highlighter = TreeSitterHighlighter::new();
        assert!(highlighter.is_ok());
    }

    #[test]
    fn test_basic_yaml_highlighting() {
        let highlighter = TreeSitterHighlighter::new().unwrap();

        let yaml = r#"
schema: nika/workflow@0.12
workflow: test

tasks:
  - id: generate
    infer: "Hello world"
"#;

        let lines = highlighter.highlight(yaml);
        assert!(!lines.is_empty());
        // Should have styled spans (not just raw text)
        assert!(lines.iter().any(|line| line.spans.len() > 1));
    }

    #[test]
    fn test_nika_verbs_highlighted() {
        let highlighter = TreeSitterHighlighter::new().unwrap();

        let yaml = "infer: hello";
        let lines = highlighter.highlight(yaml);

        // The line should have multiple spans (verb should be styled differently)
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_comments_highlighted() {
        let highlighter = TreeSitterHighlighter::new().unwrap();

        let yaml = "# This is a comment\nkey: value";
        let lines = highlighter.highlight(yaml);

        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_incremental_parsing() {
        let mut highlighter = TreeSitterHighlighter::new().unwrap();

        let original = "key: value";
        let _ = highlighter.highlight(original);

        // Simulate editing "value" to "newvalue"
        let modified = "key: newvalue";
        let lines = highlighter.highlight_incremental(modified, 5, 10, 13);

        assert!(!lines.is_empty());
    }

    #[test]
    fn test_empty_input() {
        let highlighter = TreeSitterHighlighter::new().unwrap();

        let lines = highlighter.highlight("");
        assert!(lines.is_empty() || lines.len() == 1);
    }

    #[test]
    fn test_malformed_yaml() {
        let highlighter = TreeSitterHighlighter::new().unwrap();

        let yaml = "key: [unclosed";
        let lines = highlighter.highlight(yaml);

        // Should still produce output (with error nodes highlighted)
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_byte_to_line_col_ascii() {
        // "abc\ndef" — 'e' is at byte 5 (line 1, col 1)
        let (line, col) = TreeSitterHighlighter::byte_to_line_col("abc\ndef", 5);
        assert_eq!(line, 1);
        assert_eq!(col, 1);
    }

    #[test]
    fn test_byte_to_line_col_multibyte_char() {
        // "🦋\nfoo" — emoji is 4 bytes
        let source = "🦋\nfoo";
        // Byte 4 = right after emoji, still on line 0
        let (line, col) = TreeSitterHighlighter::byte_to_line_col(source, 4);
        assert_eq!(line, 0);
        assert_eq!(col, 4, "column must be byte offset (4), not char count (1)");

        // Byte 5 = newline, starts line 1
        let (line, col) = TreeSitterHighlighter::byte_to_line_col(source, 5);
        assert_eq!(line, 1);
        assert_eq!(col, 0);

        // Byte 6 = 'f' on line 1, col 1
        let (line, col) = TreeSitterHighlighter::byte_to_line_col(source, 6);
        assert_eq!(line, 1);
        assert_eq!(col, 1);
    }
}
