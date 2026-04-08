// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Line/column and AST span indexing for LSP position mapping.
//!
//! [`LineIndex`] maps byte offsets to (line, column) positions.
//! [`PositionIndex`] maps byte offsets to AST context (which span contains this offset).

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Safely convert a `usize` to `u32`, clamping to `u32::MAX` on overflow.
///
/// In practice, files processed by the LSP are bounded by
/// [`crate::db::MAX_FILE_SIZE`] (64 MB), so this never actually clamps.
/// The function exists as a safety net for edge cases (e.g. tree-sitter
/// byte offsets or external callers).
pub(crate) fn safe_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

// ---------------------------------------------------------------------------
// LineIndex
// ---------------------------------------------------------------------------

/// Maps byte offsets to line/column positions.
/// Built once per document version, then immutable.
///
/// O(n) to build, O(log n) per lookup.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of the start of each line.
    /// `line_starts[0] = 0` always.
    line_starts: Vec<u32>,
    /// Total length of the text in bytes.
    len: u32,
}

impl LineIndex {
    /// Build a `LineIndex` from text. O(n) scan.
    ///
    /// If `text.len()` exceeds `u32::MAX`, offsets are clamped via
    /// `safe_u32`. In practice the caller (`WorldDatabase::set_text`)
    /// rejects files larger than 64 MB, so this is purely defensive.
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0u32];
        for (i, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(safe_u32(i + 1));
            }
        }
        Self {
            line_starts,
            len: safe_u32(text.len()),
        }
    }

    /// Convert byte offset to (line, col). O(log n).
    /// Both line and col are 0-based.
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let offset = offset.min(self.len);
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let col = offset - self.line_starts[line];
        (line as u32, col)
    }

    /// Convert (line, col) to byte offset. O(1).
    pub fn offset(&self, line: u32, col: u32) -> u32 {
        let line = (line as usize).min(self.line_starts.len() - 1);
        let line_start = self.line_starts[line];
        let next_line_start = self.line_starts.get(line + 1).copied().unwrap_or(self.len);
        let max_col = next_line_start - line_start;
        line_start + col.min(max_col)
    }

    /// Number of lines.
    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// Convert to `ls_types::Position` (for LSP protocol).
    pub fn to_lsp_position(&self, offset: u32) -> ls_types::Position {
        let (line, character) = self.line_col(offset);
        ls_types::Position::new(line, character)
    }

    /// Convert from `ls_types::Position` to byte offset.
    pub fn from_lsp_position(&self, pos: &ls_types::Position) -> u32 {
        self.offset(pos.line, pos.character)
    }
}

// ---------------------------------------------------------------------------
// PositionIndex
// ---------------------------------------------------------------------------

/// What kind of AST node is at a given position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanKind {
    /// The `schema:` declaration.
    Schema,
    /// The workflow's top-level name.
    WorkflowName,
    /// A task block (e.g. `summarize:`).
    Task(String),
    /// A verb within a task (e.g. `infer:`, `exec:`).
    Verb { task_id: String, verb: String },
    /// A `with:` block within a task.
    WithBlock { task_id: String },
    /// A single binding inside `with:`.
    WithBinding { task_id: String, alias: String },
    /// A `depends_on:` list within a task.
    DependsOn { task_id: String },
    /// An MCP server declaration.
    McpServer(String),
    /// A context file reference.
    ContextFile(String),
    /// An `import:` directive.
    Import { path: String },
    /// Content / prompt text inside a task.
    Content { task_id: String },
    /// A `{{template}}` expression within a task.
    Template { task_id: String, expr: String },
    /// A `guardrails:` block.
    Guardrails { task_id: String },
    /// A `for_each:` block.
    ForEach { task_id: String },
}

/// An entry in the position index.
#[derive(Debug, Clone)]
pub struct SpanEntry {
    /// Inclusive start byte offset.
    pub start: u32,
    /// Exclusive end byte offset.
    pub end: u32,
    /// The AST context for this span.
    pub kind: SpanKind,
}

/// Sorted index for O(log n) context-at-offset lookup.
///
/// Entries are sorted by `(start ASC, span_length ASC)` so the binary search
/// finds candidates quickly and the linear scan picks the innermost.
#[derive(Debug, Clone)]
pub struct PositionIndex {
    entries: Vec<SpanEntry>,
}

impl PositionIndex {
    /// Create an empty index (no spans).
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Build from a list of entries. Sorts by `(start ASC, span_length ASC)`.
    pub fn from_entries(mut entries: Vec<SpanEntry>) -> Self {
        entries.sort_unstable_by(|a, b| {
            a.start.cmp(&b.start).then_with(|| {
                a.end
                    .saturating_sub(a.start)
                    .cmp(&b.end.saturating_sub(b.start))
            })
        });
        Self { entries }
    }

    /// Find the innermost span containing the given offset. O(log n + k).
    ///
    /// Returns the smallest span whose `[start, end)` range includes `offset`.
    pub fn context_at(&self, offset: u32) -> Option<&SpanEntry> {
        // Find first entry where start > offset -- all candidates are before that.
        let idx = self.entries.partition_point(|e| e.start <= offset);

        let mut best: Option<&SpanEntry> = None;
        let mut best_len = u32::MAX;

        // Walk backwards through entries whose start <= offset.
        for entry in self.entries[..idx].iter().rev() {
            if entry.end <= offset {
                continue;
            }
            let len = entry.end - entry.start;
            if len < best_len {
                best_len = len;
                best = Some(entry);
            }
        }
        best
    }

    /// Get all entries (for iteration).
    pub fn entries(&self) -> &[SpanEntry] {
        &self.entries
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    // -----------------------------------------------------------------------
    // safe_u32 tests
    // -----------------------------------------------------------------------

    mod safe_u32_tests {
        use crate::position::safe_u32;

        #[test]
        fn fits_in_u32() {
            assert_eq!(safe_u32(0), 0);
            assert_eq!(safe_u32(42), 42);
            assert_eq!(safe_u32(u32::MAX as usize), u32::MAX);
        }

        #[test]
        fn overflows_clamps_to_max() {
            assert_eq!(safe_u32(u32::MAX as usize + 1), u32::MAX);
            assert_eq!(safe_u32(usize::MAX), u32::MAX);
        }
    }

    // -----------------------------------------------------------------------
    // LineIndex tests
    // -----------------------------------------------------------------------

    mod line_index {
        use crate::position::LineIndex;
        use pretty_assertions::assert_eq;

        #[test]
        fn empty_string() {
            let idx = LineIndex::new("");
            assert_eq!(idx.line_count(), 1);
            assert_eq!(idx.line_col(0), (0, 0));
        }

        #[test]
        fn single_char() {
            let idx = LineIndex::new("a");
            assert_eq!(idx.line_count(), 1);
            assert_eq!(idx.line_col(0), (0, 0));
            assert_eq!(idx.line_col(1), (0, 1)); // clamped to len
        }

        #[test]
        fn single_line_no_newline() {
            let idx = LineIndex::new("hello");
            assert_eq!(idx.line_count(), 1);
            assert_eq!(idx.line_col(0), (0, 0));
            assert_eq!(idx.line_col(4), (0, 4));
            assert_eq!(idx.line_col(5), (0, 5)); // at len
        }

        #[test]
        fn single_line_trailing_newline() {
            let idx = LineIndex::new("hello\n");
            assert_eq!(idx.line_count(), 2);
            assert_eq!(idx.line_col(5), (0, 5)); // at the \n
            assert_eq!(idx.line_col(6), (1, 0)); // start of second (empty) line
        }

        #[test]
        fn two_lines() {
            let idx = LineIndex::new("ab\ncd");
            assert_eq!(idx.line_count(), 2);
            assert_eq!(idx.line_col(0), (0, 0)); // 'a'
            assert_eq!(idx.line_col(1), (0, 1)); // 'b'
            assert_eq!(idx.line_col(2), (0, 2)); // '\n'
            assert_eq!(idx.line_col(3), (1, 0)); // 'c'
            assert_eq!(idx.line_col(4), (1, 1)); // 'd'
        }

        #[test]
        fn three_lines() {
            let idx = LineIndex::new("aaa\nbb\nc");
            assert_eq!(idx.line_count(), 3);
            assert_eq!(idx.line_col(0), (0, 0));
            assert_eq!(idx.line_col(3), (0, 3)); // '\n'
            assert_eq!(idx.line_col(4), (1, 0)); // first 'b'
            assert_eq!(idx.line_col(6), (1, 2)); // '\n'
            assert_eq!(idx.line_col(7), (2, 0)); // 'c'
        }

        #[test]
        fn only_newlines() {
            let idx = LineIndex::new("\n\n\n");
            assert_eq!(idx.line_count(), 4);
            assert_eq!(idx.line_col(0), (0, 0));
            assert_eq!(idx.line_col(1), (1, 0));
            assert_eq!(idx.line_col(2), (2, 0));
            assert_eq!(idx.line_col(3), (3, 0));
        }

        #[test]
        fn crlf_counts_newline_only() {
            // \r\n: the \r is part of the column, \n triggers the line break
            let idx = LineIndex::new("ab\r\ncd");
            assert_eq!(idx.line_count(), 2);
            assert_eq!(idx.line_col(0), (0, 0)); // 'a'
            assert_eq!(idx.line_col(1), (0, 1)); // 'b'
            assert_eq!(idx.line_col(2), (0, 2)); // '\r'
            assert_eq!(idx.line_col(3), (0, 3)); // '\n'
            assert_eq!(idx.line_col(4), (1, 0)); // 'c'
        }

        #[test]
        fn crlf_multiple_lines() {
            let idx = LineIndex::new("a\r\nb\r\nc");
            assert_eq!(idx.line_count(), 3);
            assert_eq!(idx.line_col(0), (0, 0)); // 'a'
            assert_eq!(idx.line_col(3), (1, 0)); // 'b'
            assert_eq!(idx.line_col(6), (2, 0)); // 'c'
        }

        #[test]
        fn offset_out_of_bounds_clamps() {
            let idx = LineIndex::new("ab");
            assert_eq!(idx.line_col(100), (0, 2)); // clamped to len=2
        }

        #[test]
        fn offset_at_line_boundary() {
            let idx = LineIndex::new("hello\nworld");
            // Offset 5 is the '\n' character itself -- still on line 0
            assert_eq!(idx.line_col(5), (0, 5));
            // Offset 6 is 'w' -- start of line 1
            assert_eq!(idx.line_col(6), (1, 0));
        }

        #[test]
        fn roundtrip_offset_linecol_offset() {
            let text = "schema: '@0.12'\ntasks:\n  summarize:\n    infer: openai/gpt-4\n";
            let idx = LineIndex::new(text);
            for offset in 0..text.len() as u32 {
                let (line, col) = idx.line_col(offset);
                let back = idx.offset(line, col);
                assert_eq!(back, offset, "roundtrip failed at offset {offset}");
            }
        }

        #[test]
        fn roundtrip_linecol_offset_linecol() {
            let text = "first\nsecond\nthird\n";
            let idx = LineIndex::new(text);
            for line in 0..idx.line_count() {
                // Find max col for this line
                let line_start = idx.offset(line, 0);
                let line_end = if line + 1 < idx.line_count() {
                    idx.offset(line + 1, 0)
                } else {
                    text.len() as u32
                };
                for col in 0..(line_end - line_start) {
                    let off = idx.offset(line, col);
                    let (l2, c2) = idx.line_col(off);
                    assert_eq!((l2, c2), (line, col));
                }
            }
        }

        #[test]
        fn offset_line_out_of_bounds() {
            let idx = LineIndex::new("ab\ncd");
            // Line 99 is way past end -- clamps to last line
            let off = idx.offset(99, 0);
            assert_eq!(off, 3); // start of last line "cd"
        }

        #[test]
        fn offset_col_out_of_bounds() {
            let idx = LineIndex::new("ab\ncd");
            // Col 99 on line 0 -- clamps to end of line (3, i.e. the \n + 1)
            let off = idx.offset(0, 99);
            assert_eq!(off, 3); // "ab\n" has 3 bytes, so max col = 3
        }

        #[test]
        fn lsp_position_conversion() {
            let idx = LineIndex::new("hello\nworld\n");
            let pos = idx.to_lsp_position(6); // 'w' in "world"
            assert_eq!(pos.line, 1);
            assert_eq!(pos.character, 0);

            let back = idx.from_lsp_position(&pos);
            assert_eq!(back, 6);
        }

        #[test]
        fn lsp_position_end_of_line() {
            let idx = LineIndex::new("abc\ndef");
            let pos = idx.to_lsp_position(3); // '\n'
            assert_eq!(pos.line, 0);
            assert_eq!(pos.character, 3);
        }

        #[test]
        fn lsp_position_roundtrip() {
            let text = "schema: '@0.12'\ntasks:\n  summarize:\n    infer: openai/gpt-4";
            let idx = LineIndex::new(text);
            for offset in 0..text.len() as u32 {
                let pos = idx.to_lsp_position(offset);
                let back = idx.from_lsp_position(&pos);
                assert_eq!(back, offset, "LSP roundtrip failed at offset {offset}");
            }
        }

        #[test]
        fn yaml_workflow_multiline() {
            let yaml = "\
schema: '@0.12'
tasks:
  fetch_data:
    fetch:
      url: https://api.example.com/data
  process:
    exec: python process.py
";
            let idx = LineIndex::new(yaml);
            assert_eq!(idx.line_count(), 8); // 7 lines + trailing newline

            // "schema" is at offset 0, line 0
            assert_eq!(idx.line_col(0), (0, 0));

            // "tasks:" starts at line 1
            let tasks_offset = yaml.find("tasks:").unwrap() as u32;
            assert_eq!(idx.line_col(tasks_offset).0, 1);
        }

        #[test]
        fn empty_lines_between_content() {
            let text = "a\n\n\nb";
            let idx = LineIndex::new(text);
            assert_eq!(idx.line_count(), 4);
            assert_eq!(idx.line_col(0), (0, 0)); // 'a'
            assert_eq!(idx.line_col(2), (1, 0)); // empty line
            assert_eq!(idx.line_col(3), (2, 0)); // empty line
            assert_eq!(idx.line_col(4), (3, 0)); // 'b'
        }

        #[test]
        fn unicode_text_byte_offsets() {
            // Multi-byte UTF-8: each character may be > 1 byte.
            // LineIndex works on byte offsets, not char offsets.
            let text = "hi\n\u{00e9}\n"; // "hi\n<e-acute>\n"
            let idx = LineIndex::new(text);
            assert_eq!(idx.line_count(), 3);
            assert_eq!(idx.line_col(3), (1, 0)); // start of second line
                                                 // e-acute is 2 bytes in UTF-8
            assert_eq!(idx.line_col(5), (1, 2)); // '\n' after e-acute
            assert_eq!(idx.line_col(6), (2, 0)); // empty third line
        }

        #[test]
        fn long_line() {
            let line = "x".repeat(10_000);
            let text = format!("{line}\n{line}");
            let idx = LineIndex::new(&text);
            assert_eq!(idx.line_count(), 2);
            assert_eq!(idx.line_col(9_999), (0, 9_999));
            assert_eq!(idx.line_col(10_001), (1, 0));
        }

        #[test]
        fn offset_zero_line_zero_col_zero() {
            // For any text, offset 0 should always be (0, 0)
            for text in &["", "a", "\n", "abc\ndef", "\n\n\n"] {
                let idx = LineIndex::new(text);
                assert_eq!(idx.line_col(0), (0, 0), "failed for {text:?}");
            }
        }

        #[test]
        fn last_byte_of_text() {
            let text = "abc";
            let idx = LineIndex::new(text);
            // offset 2 = 'c', offset 3 = at len (clamped)
            assert_eq!(idx.line_col(2), (0, 2));
            assert_eq!(idx.line_col(3), (0, 3));
        }

        #[test]
        fn stress_many_short_lines() {
            let text: String = (0..1000).map(|_| "x\n").collect();
            let idx = LineIndex::new(&text);
            assert_eq!(idx.line_count(), 1001); // 1000 lines + trailing empty
            assert_eq!(idx.line_col(0), (0, 0));
            assert_eq!(idx.line_col(2), (1, 0)); // second line
            assert_eq!(idx.line_col(1998), (999, 0)); // line 999
        }
    }

    // -----------------------------------------------------------------------
    // LineIndex proptest
    // -----------------------------------------------------------------------

    mod line_index_proptest {
        use crate::position::LineIndex;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn roundtrip_offset(text in ".{0,200}") {
                let idx = LineIndex::new(&text);
                for offset in 0..text.len() as u32 {
                    let (line, col) = idx.line_col(offset);
                    let back = idx.offset(line, col);
                    prop_assert_eq!(back, offset,
                        "roundtrip failed for text={:?} offset={}", text, offset);
                }
            }

            #[test]
            fn out_of_bounds_clamps(text in ".{0,50}", extra_offset in 0u32..1000) {
                let idx = LineIndex::new(&text);
                let offset = text.len() as u32 + extra_offset;
                let (line, col) = idx.line_col(offset);
                // Should clamp to len
                let back = idx.offset(line, col);
                prop_assert!(back <= text.len() as u32);
            }

            #[test]
            fn line_col_monotonic(text in ".{1,200}") {
                let idx = LineIndex::new(&text);
                let mut prev = (0u32, 0u32);
                for offset in 0..text.len() as u32 {
                    let (line, col) = idx.line_col(offset);
                    // (line, col) should be monotonically non-decreasing
                    // when we go in a new line col resets, but line increases
                    if line == prev.0 {
                        prop_assert!(col >= prev.1,
                            "col decreased on same line at offset={offset}");
                    } else {
                        prop_assert!(line > prev.0,
                            "line decreased at offset={offset}");
                    }
                    prev = (line, col);
                }
            }

            #[test]
            fn line_count_equals_newlines_plus_one(text in ".{0,100}") {
                let idx = LineIndex::new(&text);
                let newlines = text.bytes().filter(|&b| b == b'\n').count() as u32;
                prop_assert_eq!(idx.line_count(), newlines + 1);
            }

            #[test]
            fn lsp_roundtrip(text in ".{0,100}") {
                let idx = LineIndex::new(&text);
                for offset in 0..text.len() as u32 {
                    let pos = idx.to_lsp_position(offset);
                    let back = idx.from_lsp_position(&pos);
                    prop_assert_eq!(back, offset,
                        "LSP roundtrip failed at offset={}", offset);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // PositionIndex tests
    // -----------------------------------------------------------------------

    mod position_index {
        use crate::position::*;
        use pretty_assertions::assert_eq;

        fn task_entry(start: u32, end: u32, name: &str) -> SpanEntry {
            SpanEntry {
                start,
                end,
                kind: SpanKind::Task(name.to_string()),
            }
        }

        fn verb_entry(start: u32, end: u32, task: &str, verb: &str) -> SpanEntry {
            SpanEntry {
                start,
                end,
                kind: SpanKind::Verb {
                    task_id: task.to_string(),
                    verb: verb.to_string(),
                },
            }
        }

        fn with_binding_entry(start: u32, end: u32, task: &str, alias: &str) -> SpanEntry {
            SpanEntry {
                start,
                end,
                kind: SpanKind::WithBinding {
                    task_id: task.to_string(),
                    alias: alias.to_string(),
                },
            }
        }

        #[test]
        fn empty_index() {
            let idx = PositionIndex::empty();
            assert!(idx.is_empty());
            assert_eq!(idx.len(), 0);
            assert!(idx.context_at(0).is_none());
            assert!(idx.context_at(100).is_none());
        }

        #[test]
        fn single_entry_inside() {
            let idx = PositionIndex::from_entries(vec![task_entry(10, 20, "task1")]);
            let ctx = idx.context_at(15).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("task1".to_string()));
        }

        #[test]
        fn single_entry_at_start() {
            let idx = PositionIndex::from_entries(vec![task_entry(10, 20, "task1")]);
            let ctx = idx.context_at(10).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("task1".to_string()));
        }

        #[test]
        fn single_entry_at_end_exclusive() {
            // end is exclusive, so offset == end is NOT inside
            let idx = PositionIndex::from_entries(vec![task_entry(10, 20, "task1")]);
            assert!(idx.context_at(20).is_none());
        }

        #[test]
        fn single_entry_before() {
            let idx = PositionIndex::from_entries(vec![task_entry(10, 20, "task1")]);
            assert!(idx.context_at(5).is_none());
        }

        #[test]
        fn single_entry_after() {
            let idx = PositionIndex::from_entries(vec![task_entry(10, 20, "task1")]);
            assert!(idx.context_at(25).is_none());
        }

        #[test]
        fn nested_returns_innermost() {
            // Outer: task [0, 100), Inner: verb [20, 50)
            let idx = PositionIndex::from_entries(vec![
                task_entry(0, 100, "task1"),
                verb_entry(20, 50, "task1", "infer"),
            ]);
            // At offset 30 -- should return the verb (innermost)
            let ctx = idx.context_at(30).unwrap();
            assert_eq!(
                ctx.kind,
                SpanKind::Verb {
                    task_id: "task1".to_string(),
                    verb: "infer".to_string()
                }
            );
        }

        #[test]
        fn nested_outside_inner_returns_outer() {
            let idx = PositionIndex::from_entries(vec![
                task_entry(0, 100, "task1"),
                verb_entry(20, 50, "task1", "infer"),
            ]);
            // At offset 5 -- only the task contains it
            let ctx = idx.context_at(5).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("task1".to_string()));
        }

        #[test]
        fn multiple_non_overlapping() {
            let idx = PositionIndex::from_entries(vec![
                task_entry(0, 30, "task1"),
                task_entry(40, 70, "task2"),
                task_entry(80, 110, "task3"),
            ]);

            let ctx = idx.context_at(10).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("task1".to_string()));

            let ctx = idx.context_at(50).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("task2".to_string()));

            let ctx = idx.context_at(90).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("task3".to_string()));

            // Between spans
            assert!(idx.context_at(35).is_none());
        }

        #[test]
        fn three_levels_nested() {
            let idx = PositionIndex::from_entries(vec![
                task_entry(0, 200, "task1"),
                verb_entry(10, 100, "task1", "infer"),
                with_binding_entry(30, 60, "task1", "alias1"),
            ]);

            // Deep inside: should return with_binding (smallest)
            let ctx = idx.context_at(40).unwrap();
            assert_eq!(
                ctx.kind,
                SpanKind::WithBinding {
                    task_id: "task1".to_string(),
                    alias: "alias1".to_string()
                }
            );

            // Inside verb but outside binding
            let ctx = idx.context_at(70).unwrap();
            assert_eq!(
                ctx.kind,
                SpanKind::Verb {
                    task_id: "task1".to_string(),
                    verb: "infer".to_string()
                }
            );

            // Inside task but outside verb
            let ctx = idx.context_at(150).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("task1".to_string()));
        }

        #[test]
        fn adjacent_spans() {
            // Two spans touching: [0,10) and [10, 20)
            let idx = PositionIndex::from_entries(vec![
                task_entry(0, 10, "task1"),
                task_entry(10, 20, "task2"),
            ]);

            let ctx = idx.context_at(9).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("task1".to_string()));

            let ctx = idx.context_at(10).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("task2".to_string()));
        }

        #[test]
        fn unsorted_input_gets_sorted() {
            // Provide entries in reverse order -- from_entries should sort them.
            let idx = PositionIndex::from_entries(vec![
                task_entry(40, 70, "task2"),
                task_entry(0, 30, "task1"),
            ]);

            let ctx = idx.context_at(10).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("task1".to_string()));

            let ctx = idx.context_at(50).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("task2".to_string()));
        }

        #[test]
        fn entries_accessor() {
            let entries = vec![task_entry(0, 30, "a"), task_entry(40, 70, "b")];
            let idx = PositionIndex::from_entries(entries);
            assert_eq!(idx.entries().len(), 2);
            assert_eq!(idx.len(), 2);
            assert!(!idx.is_empty());
        }

        #[test]
        fn schema_span() {
            let idx = PositionIndex::from_entries(vec![SpanEntry {
                start: 0,
                end: 15,
                kind: SpanKind::Schema,
            }]);
            let ctx = idx.context_at(5).unwrap();
            assert_eq!(ctx.kind, SpanKind::Schema);
        }

        #[test]
        fn all_span_kinds() {
            // Ensure each SpanKind variant can be stored and retrieved.
            let entries = vec![
                SpanEntry {
                    start: 0,
                    end: 10,
                    kind: SpanKind::Schema,
                },
                SpanEntry {
                    start: 10,
                    end: 20,
                    kind: SpanKind::WorkflowName,
                },
                SpanEntry {
                    start: 20,
                    end: 30,
                    kind: SpanKind::Task("t".into()),
                },
                SpanEntry {
                    start: 30,
                    end: 40,
                    kind: SpanKind::Verb {
                        task_id: "t".into(),
                        verb: "infer".into(),
                    },
                },
                SpanEntry {
                    start: 40,
                    end: 50,
                    kind: SpanKind::WithBlock {
                        task_id: "t".into(),
                    },
                },
                SpanEntry {
                    start: 50,
                    end: 60,
                    kind: SpanKind::WithBinding {
                        task_id: "t".into(),
                        alias: "a".into(),
                    },
                },
                SpanEntry {
                    start: 60,
                    end: 70,
                    kind: SpanKind::DependsOn {
                        task_id: "t".into(),
                    },
                },
                SpanEntry {
                    start: 70,
                    end: 80,
                    kind: SpanKind::McpServer("s".into()),
                },
                SpanEntry {
                    start: 80,
                    end: 90,
                    kind: SpanKind::ContextFile("f".into()),
                },
                SpanEntry {
                    start: 90,
                    end: 100,
                    kind: SpanKind::Import { path: "p".into() },
                },
                SpanEntry {
                    start: 100,
                    end: 110,
                    kind: SpanKind::Content {
                        task_id: "t".into(),
                    },
                },
                SpanEntry {
                    start: 110,
                    end: 120,
                    kind: SpanKind::Template {
                        task_id: "t".into(),
                        expr: "e".into(),
                    },
                },
                SpanEntry {
                    start: 120,
                    end: 130,
                    kind: SpanKind::Guardrails {
                        task_id: "t".into(),
                    },
                },
                SpanEntry {
                    start: 130,
                    end: 140,
                    kind: SpanKind::ForEach {
                        task_id: "t".into(),
                    },
                },
            ];
            let idx = PositionIndex::from_entries(entries);
            assert_eq!(idx.len(), 14);

            // Spot-check a few
            assert_eq!(idx.context_at(5).unwrap().kind, SpanKind::Schema);
            assert_eq!(
                idx.context_at(75).unwrap().kind,
                SpanKind::McpServer("s".into())
            );
            assert_eq!(
                idx.context_at(135).unwrap().kind,
                SpanKind::ForEach {
                    task_id: "t".into()
                }
            );
        }

        #[test]
        fn zero_length_span_never_matches() {
            let idx = PositionIndex::from_entries(vec![SpanEntry {
                start: 10,
                end: 10,
                kind: SpanKind::Schema,
            }]);
            assert!(idx.context_at(10).is_none());
        }

        #[test]
        fn many_overlapping_spans_returns_smallest() {
            // 5 concentric spans
            let entries: Vec<SpanEntry> = (0..5)
                .map(|i| {
                    let margin = i * 10;
                    task_entry(margin, 100 - margin, &format!("t{i}"))
                })
                .collect();
            let idx = PositionIndex::from_entries(entries);

            // At center (50), the innermost is t4: [40, 60)
            let ctx = idx.context_at(50).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("t4".to_string()));
        }

        #[test]
        fn offset_at_zero_with_span_starting_at_zero() {
            let idx = PositionIndex::from_entries(vec![task_entry(0, 100, "root")]);
            let ctx = idx.context_at(0).unwrap();
            assert_eq!(ctx.kind, SpanKind::Task("root".to_string()));
        }

        #[test]
        fn context_at_with_gap_between_spans() {
            let idx =
                PositionIndex::from_entries(vec![task_entry(0, 10, "a"), task_entry(50, 60, "b")]);
            // In the gap
            assert!(idx.context_at(25).is_none());
        }

        #[test]
        fn large_offset_no_panic() {
            let idx = PositionIndex::from_entries(vec![task_entry(0, 10, "a")]);
            assert!(idx.context_at(u32::MAX).is_none());
        }
    }
}
