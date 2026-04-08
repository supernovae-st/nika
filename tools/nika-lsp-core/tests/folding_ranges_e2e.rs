// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! End-to-end tests for folding ranges via the LspHandler trait.

use nika_lsp_core::handler::{DefaultHandler, LspHandler};
use nika_lsp_core::handlers::folding_ranges::{FoldEntry, FoldKind};

fn fold_lines(text: &str) -> Vec<(u32, u32)> {
    let handler = DefaultHandler::new();
    handler
        .folding_ranges(text)
        .into_iter()
        .map(|r| (r.start_line, r.end_line))
        .collect()
}

fn fold_entries(text: &str) -> Vec<FoldEntry> {
    let handler = DefaultHandler::new();
    handler.folding_ranges(text)
}

// ════════════════════════════════════════════════════════════════════════════
// 1. Single task fold
// ════════════════════════════════════════════════════════════════════════════

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
        "tasks: block should fold lines 0-3: {:?}",
        folds
    );
    assert!(
        folds.contains(&(1, 3)),
        "task block should fold lines 1-3: {:?}",
        folds
    );
}

// ════════════════════════════════════════════════════════════════════════════
// 2. Multiple tasks fold
// ════════════════════════════════════════════════════════════════════════════

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
        "first task should fold lines 1-2: {:?}",
        folds
    );
    assert!(
        folds.contains(&(3, 4)),
        "second task should fold lines 3-4: {:?}",
        folds
    );
}

// ════════════════════════════════════════════════════════════════════════════
// 3. MCP config section fold
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn mcp_config_section_fold() {
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

// ════════════════════════════════════════════════════════════════════════════
// 4. Multi-line string fold (prompt: |)
// ════════════════════════════════════════════════════════════════════════════

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
        "multi-line string should fold lines 3-6: {:?}",
        folds
    );
}

#[test]
fn multiline_string_fold_chomping_variants() {
    // |+ keep, |- strip, >+ keep, >- strip
    for indicator in &["|+", "|-", ">+", ">-", "|", ">"] {
        let text = format!(
            "\
tasks:
  - id: s1
    infer:
      prompt: {}
        line one
        line two",
            indicator
        );
        let folds = fold_lines(&text);
        assert!(
            folds.contains(&(3, 5)),
            "block scalar '{}' should fold lines 3-5: {:?}",
            indicator,
            folds
        );
    }
}

// ════════════════════════════════════════════════════════════════════════════
// 5. Comment block fold
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn comment_block_fold() {
    let text = "\
# This is a header comment
# that spans multiple lines
# explaining the workflow
schema: nika/workflow@0.12";

    let entries = fold_entries(text);
    let comment_folds: Vec<_> = entries
        .iter()
        .filter(|e| e.kind == FoldKind::Comment)
        .collect();
    assert_eq!(comment_folds.len(), 1, "should have exactly 1 comment fold");
    assert_eq!(comment_folds[0].start_line, 0);
    assert_eq!(comment_folds[0].end_line, 2);
}

#[test]
fn single_comment_line_no_fold() {
    let text = "\
# Just one comment
schema: nika/workflow@0.12";

    let entries = fold_entries(text);
    let comment_folds: Vec<_> = entries
        .iter()
        .filter(|e| e.kind == FoldKind::Comment)
        .collect();
    assert!(
        comment_folds.is_empty(),
        "single comment should not fold: {:?}",
        comment_folds
    );
}

// ════════════════════════════════════════════════════════════════════════════
// 6. Empty document
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn empty_document() {
    let handler = DefaultHandler::new();
    assert!(handler.folding_ranges("").is_empty());
    assert!(handler.folding_ranges("   \n  \n").is_empty());
    assert!(handler.folding_ranges("\n\n\n").is_empty());
}

// ════════════════════════════════════════════════════════════════════════════
// 7. Full workflow integration
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn full_workflow_integration() {
    let text = "\
schema: nika/workflow@0.12
workflow: integration-test

mcp:
  servers:
    novanet:
      command: node

context:
  files:
    brand: ./brand.md

tasks:
  - id: generate
    with:
      data: $import
    infer:
      prompt: |
        Generate content
        using brand guidelines
  - id: publish
    exec: \"echo done\"";

    let handler = DefaultHandler::new();
    let folds = handler.folding_ranges(text);
    assert!(
        folds.len() >= 8,
        "full workflow should have many folds, got {}: {:?}",
        folds.len(),
        folds
            .iter()
            .map(|r| (r.start_line, r.end_line))
            .collect::<Vec<_>>()
    );

    // Verify sorted
    for w in folds.windows(2) {
        assert!(
            w[0].start_line <= w[1].start_line,
            "folds should be sorted by start_line"
        );
    }
}
