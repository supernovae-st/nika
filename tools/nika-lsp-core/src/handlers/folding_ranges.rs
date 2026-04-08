// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Folding Ranges handler — protocol-agnostic.
//!
//! Provides folding ranges for YAML workflow elements:
//! - Task blocks (from `- id:` to next task or end)
//! - `with:` binding blocks
//! - `mcp:` server configuration sections
//! - Multi-line strings (`|` or `>`)
//! - `content:` arrays
//! - Top-level sections (`tasks:`, `mcp:`, `context:`, etc.)
//! - Consecutive comment blocks

/// The kind of folding range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FoldKind {
    Region,
    Comment,
}

/// A protocol-agnostic folding range entry.
#[derive(Debug, Clone)]
pub struct FoldEntry {
    pub start_line: u32,
    pub end_line: u32,
    pub kind: FoldKind,
}

/// Compute folding ranges for the document.
///
/// Uses line-by-line indent tracking to detect:
/// 1. Task boundaries (`- id:` blocks)
/// 2. Key-value blocks (any `key:` without inline value)
/// 3. Multi-line strings (`|` or `>` after `:`)
/// 4. Comment blocks (consecutive `#` lines)
pub fn folding_ranges(text: &str) -> Vec<FoldEntry> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();

    // Pass 1: Detect indent-based block folds (sections, tasks, nested blocks)
    detect_block_folds(&lines, &mut ranges);

    // Pass 2: Detect multi-line string folds (| and >)
    detect_multiline_string_folds(&lines, &mut ranges);

    // Pass 3: Detect comment block folds
    detect_comment_folds(&lines, &mut ranges);

    // Stable sort by start_line for deterministic output
    ranges.sort_by_key(|r| r.start_line);

    ranges
}

/// Measure the indentation level of a line (number of leading spaces).
/// Returns `None` for blank lines (they don't affect block boundaries).
fn indent_level(line: &str) -> Option<usize> {
    if line.trim().is_empty() {
        return None;
    }
    Some(line.len() - line.trim_start().len())
}

/// Check whether a trimmed line has a value on the same line after the colon.
/// A `key:` with no value (or only whitespace) starts a block fold.
/// A `key: value` does not start a block fold on its own.
fn has_inline_value(trimmed: &str) -> bool {
    if let Some(colon_pos) = trimmed.find(':') {
        let after = &trimmed[colon_pos + 1..];
        let after_trimmed = after.trim();
        // Has an inline value if there's non-empty text after the colon
        // that is NOT a block scalar indicator (| or >)
        if after_trimmed.is_empty() {
            return false;
        }
        // Block scalar indicators start a multi-line string, not an inline value
        if after_trimmed == "|"
            || after_trimmed == ">"
            || after_trimmed.starts_with("| ")
            || after_trimmed.starts_with("> ")
            || after_trimmed.starts_with("|+")
            || after_trimmed.starts_with("|-")
            || after_trimmed.starts_with(">+")
            || after_trimmed.starts_with(">-")
        {
            return false;
        }
        return true;
    }
    // No colon at all — not a key-value line, cannot have an inline value
    false
}

/// Top-level and nested section keys that commonly start foldable blocks.
const SECTION_KEYS: &[&str] = &[
    "tasks:", "mcp:", "context:", "include:", "skills:", "servers:", "files:",
];

/// Detect indent-based block folds.
///
/// Tracks a stack of `(indent_level, start_line)` pairs. When indent decreases
/// back to or below a stacked level, the block is closed.
fn detect_block_folds(lines: &[&str], ranges: &mut Vec<FoldEntry>) {
    // Stack entries: (indent_level, start_line_index)
    let mut stack: Vec<(usize, usize)> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let Some(indent) = indent_level(line) else {
            continue; // skip blank lines
        };

        let trimmed = line.trim();

        // Skip pure comment lines in this pass (handled by comment fold pass)
        if trimmed.starts_with('#') {
            continue;
        }

        // Close any blocks on the stack whose indent is >= current indent.
        // This means the current line is at the same level or higher (less indented),
        // so previous deeper blocks have ended.
        while let Some(&(stack_indent, stack_start)) = stack.last() {
            if indent <= stack_indent {
                // The block started at stack_start ends at the previous non-blank line
                let end_line = find_last_nonempty_line(lines, stack_start + 1, i);
                if let Some(end) = end_line {
                    if end > stack_start {
                        ranges.push(FoldEntry {
                            start_line: stack_start as u32,
                            end_line: end as u32,
                            kind: FoldKind::Region,
                        });
                    }
                }
                stack.pop();
            } else {
                break;
            }
        }

        // Determine if this line starts a new foldable block
        let starts_block = if trimmed.starts_with("- id:") {
            // Task item always starts a fold
            true
        } else if SECTION_KEYS
            .iter()
            .any(|k| trimmed == *k || trimmed.starts_with(k))
        {
            // Top-level or nested section key without inline value
            !has_inline_value(trimmed)
        } else if trimmed.contains(':') && !trimmed.starts_with('-') {
            // Generic key: without inline value starts a block fold
            !has_inline_value(trimmed)
        } else {
            false
        };

        if starts_block {
            stack.push((indent, i));
        }
    }

    // Close any remaining open blocks at end of document
    let total = lines.len();
    while let Some((_, stack_start)) = stack.pop() {
        let end_line = find_last_nonempty_line(lines, stack_start + 1, total);
        if let Some(end) = end_line {
            if end > stack_start {
                ranges.push(FoldEntry {
                    start_line: stack_start as u32,
                    end_line: end as u32,
                    kind: FoldKind::Region,
                });
            }
        }
    }
}

/// Detect multi-line string folds (block scalar `|` or `>`).
///
/// A line ending with `: |`, `: >`, `: |+`, `: |-`, `: >+`, or `: >-`
/// starts a multi-line string. The fold extends through all subsequent lines
/// that are either blank or more indented than the key line.
fn detect_multiline_string_folds(lines: &[&str], ranges: &mut Vec<FoldEntry>) {
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Check for block scalar indicator after a colon
        let is_block_scalar = if let Some(colon_pos) = trimmed.find(':') {
            let after = trimmed[colon_pos + 1..].trim();
            after == "|"
                || after == ">"
                || after.starts_with("| ")
                || after.starts_with("> ")
                || after.starts_with("|+")
                || after.starts_with("|-")
                || after.starts_with(">+")
                || after.starts_with(">-")
        } else {
            false
        };

        if is_block_scalar {
            let key_indent = indent_level(lines[i]).unwrap_or(0);
            let start = i;
            i += 1;

            // Consume all lines that are blank or more indented than the key
            while i < lines.len() {
                if lines[i].trim().is_empty() {
                    i += 1;
                    continue;
                }
                let line_indent = indent_level(lines[i]).unwrap_or(0);
                if line_indent > key_indent {
                    i += 1;
                } else {
                    break;
                }
            }

            // Find the last non-empty line in the string content
            let end = find_last_nonempty_line(lines, start + 1, i);
            if let Some(end_line) = end {
                if end_line > start {
                    ranges.push(FoldEntry {
                        start_line: start as u32,
                        end_line: end_line as u32,
                        kind: FoldKind::Region,
                    });
                }
            }
        } else {
            i += 1;
        }
    }
}

/// Detect consecutive comment line folds.
///
/// Groups of 2+ consecutive lines starting with `#` form a foldable comment block.
fn detect_comment_folds(lines: &[&str], ranges: &mut Vec<FoldEntry>) {
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim().starts_with('#') {
            let start = i;
            while i < lines.len() && lines[i].trim().starts_with('#') {
                i += 1;
            }
            // Only fold if there are 2+ consecutive comment lines
            if i - start >= 2 {
                ranges.push(FoldEntry {
                    start_line: start as u32,
                    end_line: (i - 1) as u32,
                    kind: FoldKind::Comment,
                });
            }
        } else {
            i += 1;
        }
    }
}

/// Find the last non-empty line in the range `[from, to)` scanning backwards.
fn find_last_nonempty_line(lines: &[&str], from: usize, to: usize) -> Option<usize> {
    let to = to.min(lines.len());
    (from..to).rev().find(|&j| !lines[j].trim().is_empty())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn fold_lines(text: &str) -> Vec<(u32, u32)> {
        folding_ranges(text)
            .into_iter()
            .map(|r| (r.start_line, r.end_line))
            .collect()
    }

    fn fold_kinds(text: &str) -> Vec<(u32, u32, FoldKind)> {
        folding_ranges(text)
            .into_iter()
            .map(|r| (r.start_line, r.end_line, r.kind))
            .collect()
    }

    #[test]
    fn empty_document_returns_no_folds() {
        assert!(folding_ranges("").is_empty());
        assert!(folding_ranges("   \n  \n").is_empty());
    }

    #[test]
    fn single_task_fold() {
        let text = "\
tasks:
  - id: step1
    infer: \"Generate content\"
    timeout: 30";

        let folds = fold_lines(text);
        assert!(
            folds.contains(&(0, 3)),
            "tasks: block should fold: {:?}",
            folds
        );
        assert!(
            folds.contains(&(1, 3)),
            "task block should fold: {:?}",
            folds
        );
    }

    #[test]
    fn multiple_tasks_fold() {
        let text = "\
tasks:
  - id: step1
    infer: \"Hello\"
  - id: step2
    exec: \"echo done\"";

        let folds = fold_lines(text);
        assert!(
            folds.contains(&(0, 4)),
            "tasks: block should span all tasks: {:?}",
            folds
        );
        assert!(
            folds.contains(&(1, 2)),
            "first task should fold: {:?}",
            folds
        );
        assert!(
            folds.contains(&(3, 4)),
            "second task should fold: {:?}",
            folds
        );
    }

    #[test]
    fn multiline_string_fold() {
        let text = "\
tasks:
  - id: step1
    infer:
      prompt: |
        This is a long
        multi-line prompt
        that spans several lines";

        let folds = fold_lines(text);
        assert!(
            folds.contains(&(3, 6)),
            "multi-line string should fold: {:?}",
            folds
        );
    }

    #[test]
    fn comment_block_fold() {
        let text = "\
# This is a header comment
# that spans multiple lines
# explaining the workflow
schema: nika/workflow@0.12";

        let kinds = fold_kinds(text);
        assert!(
            kinds.contains(&(0, 2, FoldKind::Comment)),
            "comment block should fold with Comment kind: {:?}",
            kinds
        );
    }

    #[test]
    fn mcp_config_fold() {
        let text = "\
mcp:
  servers:
    novanet:
      command: node
      args: [\"server.js\"]
    perplexity:
      command: npx";

        let folds = fold_lines(text);
        assert!(
            folds.contains(&(0, 6)),
            "mcp: should fold entire section: {:?}",
            folds
        );
        assert!(folds.contains(&(1, 6)), "servers: should fold: {:?}", folds);
        assert!(
            folds.contains(&(2, 4)),
            "novanet: server should fold: {:?}",
            folds
        );
        assert!(
            folds.contains(&(5, 6)),
            "perplexity: server should fold: {:?}",
            folds
        );
    }

    #[test]
    fn nested_with_block_fold() {
        let text = "\
tasks:
  - id: step1
    with:
      data: $step0
      label: \"test\"
    infer: \"Use {{with.data}}\"";

        let folds = fold_lines(text);
        assert!(
            folds.contains(&(2, 4)),
            "with: block should fold: {:?}",
            folds
        );
    }

    #[test]
    fn results_sorted_by_start_line() {
        let text = "\
# comment 1
# comment 2
tasks:
  - id: step1
    infer: \"hello\"";

        let ranges = folding_ranges(text);
        for w in ranges.windows(2) {
            assert!(
                w[0].start_line <= w[1].start_line,
                "ranges should be sorted: {:?}",
                ranges.iter().map(|r| r.start_line).collect::<Vec<_>>()
            );
        }
    }
}
