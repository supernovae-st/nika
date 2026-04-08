// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Thin wrapper around tree-sitter-yaml for incremental parsing.
//!
//! [`RecoveryParser`] manages the parser lifecycle and keeps the
//! previous tree for incremental re-parsing on text edits.

use tree_sitter::{InputEdit, Parser, Tree};

/// Incremental tree-sitter YAML parser.
///
/// Holds the previous tree so that subsequent parses can be incremental
/// (only re-parsing the changed region). This is critical for LSP
/// performance on large files.
pub struct RecoveryParser {
    parser: Parser,
    old_tree: Option<Tree>,
}

impl RecoveryParser {
    /// Create a new parser initialized with the YAML grammar.
    ///
    /// # Panics
    ///
    /// Panics if the YAML language cannot be loaded (should never happen
    /// with a statically linked grammar).
    pub fn new() -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_yaml::LANGUAGE.into())
            .expect("tree-sitter-yaml language should load");
        // 5 second timeout to prevent DoS from pathological YAML inputs
        // TODO: migrate to parse_with_options when tree-sitter stabilizes the new API
        #[allow(deprecated)]
        parser.set_timeout_micros(5_000_000);
        Self {
            parser,
            old_tree: None,
        }
    }

    /// Full parse of the entire source text.
    ///
    /// Replaces any cached tree. Returns `None` only if tree-sitter
    /// itself encounters an internal error (extremely rare).
    pub fn parse(&mut self, text: &str) -> Option<Tree> {
        let tree = self.parser.parse(text, None)?;
        self.old_tree = Some(tree.clone());
        Some(tree)
    }

    /// Incremental re-parse after a text edit.
    ///
    /// The caller must provide the `InputEdit` describing what changed
    /// and the new full text. If no previous tree is cached, this falls
    /// back to a full parse.
    pub fn parse_incremental(&mut self, text: &str, edit: &InputEdit) -> Option<Tree> {
        if let Some(old) = self.old_tree.as_mut() {
            old.edit(edit);
            let tree = self.parser.parse(text, Some(old))?;
            self.old_tree = Some(tree.clone());
            Some(tree)
        } else {
            self.parse(text)
        }
    }

    /// Clear the cached tree, forcing the next parse to be a full parse.
    pub fn reset(&mut self) {
        self.old_tree = None;
    }

    /// Whether a previous tree is cached for incremental parsing.
    pub fn has_cached_tree(&self) -> bool {
        self.old_tree.is_some()
    }
}

impl Default for RecoveryParser {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for RecoveryParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryParser")
            .field("has_cached_tree", &self.old_tree.is_some())
            .finish()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_parser_has_no_cached_tree() {
        let parser = RecoveryParser::new();
        assert!(!parser.has_cached_tree());
    }

    #[test]
    fn default_parser_works() {
        let parser = RecoveryParser::default();
        assert!(!parser.has_cached_tree());
    }

    #[test]
    fn parse_valid_yaml() {
        let mut parser = RecoveryParser::new();
        let tree = parser.parse("key: value");
        assert!(tree.is_some());
        assert!(parser.has_cached_tree());
    }

    #[test]
    fn parse_empty_string() {
        let mut parser = RecoveryParser::new();
        let tree = parser.parse("").expect("should parse empty string");
        let root = tree.root_node();
        assert_eq!(root.kind(), "stream");
    }

    #[test]
    fn parse_broken_yaml_still_returns_tree() {
        let mut parser = RecoveryParser::new();
        let tree = parser.parse("{{{{ ]]]] ::::");
        assert!(tree.is_some());
        assert!(tree.unwrap().root_node().has_error());
    }

    #[test]
    fn parse_workflow_yaml() {
        let mut parser = RecoveryParser::new();
        let yaml = "schema: '@0.12'\ntasks:\n  - id: test\n    infer:\n      prompt: hello";
        let tree = parser.parse(yaml);
        assert!(tree.is_some());
        assert!(!tree.unwrap().root_node().has_error());
    }

    #[test]
    fn incremental_parse_without_cached_tree() {
        let mut parser = RecoveryParser::new();
        // No previous parse -- should fall back to full parse
        let edit = InputEdit {
            start_byte: 0,
            old_end_byte: 0,
            new_end_byte: 1,
            start_position: tree_sitter::Point { row: 0, column: 0 },
            old_end_position: tree_sitter::Point { row: 0, column: 0 },
            new_end_position: tree_sitter::Point { row: 0, column: 1 },
        };
        let tree = parser.parse_incremental("a", &edit);
        assert!(tree.is_some());
        assert!(parser.has_cached_tree());
    }

    #[test]
    fn incremental_parse_with_cached_tree() {
        let mut parser = RecoveryParser::new();

        let v1 = "key: value";
        let tree1 = parser.parse(v1).unwrap();
        assert!(!tree1.root_node().has_error());

        // Edit: "key: value" -> "key: value2"
        let edit = InputEdit {
            start_byte: 10,
            old_end_byte: 10,
            new_end_byte: 11,
            start_position: tree_sitter::Point { row: 0, column: 10 },
            old_end_position: tree_sitter::Point { row: 0, column: 10 },
            new_end_position: tree_sitter::Point { row: 0, column: 11 },
        };
        let v2 = "key: value2";
        let tree2 = parser.parse_incremental(v2, &edit);
        assert!(tree2.is_some());
        assert!(!tree2.unwrap().root_node().has_error());
    }

    #[test]
    fn incremental_parse_adding_line() {
        let mut parser = RecoveryParser::new();

        let v1 = "schema: '@0.12'";
        parser.parse(v1).unwrap();

        // Append a newline + "tasks:"
        let v2 = "schema: '@0.12'\ntasks:";
        let edit = InputEdit {
            start_byte: 15,
            old_end_byte: 15,
            new_end_byte: 22,
            start_position: tree_sitter::Point { row: 0, column: 15 },
            old_end_position: tree_sitter::Point { row: 0, column: 15 },
            new_end_position: tree_sitter::Point { row: 1, column: 6 },
        };
        let tree2 = parser.parse_incremental(v2, &edit).unwrap();
        assert!(!tree2.root_node().has_error());
    }

    #[test]
    fn reset_clears_cached_tree() {
        let mut parser = RecoveryParser::new();
        parser.parse("key: value");
        assert!(parser.has_cached_tree());
        parser.reset();
        assert!(!parser.has_cached_tree());
    }

    #[test]
    fn debug_impl() {
        let parser = RecoveryParser::new();
        let debug = format!("{parser:?}");
        assert!(debug.contains("RecoveryParser"));
        assert!(debug.contains("has_cached_tree"));
    }

    #[test]
    fn multiple_full_parses_replace_cache() {
        let mut parser = RecoveryParser::new();

        let tree1 = parser.parse("a: 1").unwrap();
        assert!(!tree1.root_node().has_error());

        let tree2 = parser.parse("b: 2\nc: 3").unwrap();
        assert!(!tree2.root_node().has_error());

        // Cache should hold the second tree
        assert!(parser.has_cached_tree());
    }

    #[test]
    fn parse_large_document() {
        let mut parser = RecoveryParser::new();
        let mut yaml = String::from("schema: '@0.12'\ntasks:\n");
        for i in 0..100 {
            yaml.push_str(&format!(
                "  - id: task_{i}\n    infer:\n      prompt: \"Task {i}\"\n"
            ));
        }
        let tree = parser.parse(&yaml);
        assert!(tree.is_some());
        assert!(!tree.unwrap().root_node().has_error());
    }
}
