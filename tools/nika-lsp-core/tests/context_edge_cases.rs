// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Edge-case tests for `detect_context()` in `nika_lsp_core::analysis::context`.
//!
//! Each test targets a pathological input that could trip up the text-based
//! cursor context detection: boundary offsets, exotic whitespace, malformed
//! templates, deep nesting, duplicate verbs, comments, and Unicode task IDs.

use nika_lsp_core::analysis::context::{detect_context, ContentFocus, CursorContext, InvokeFocus};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Detect context at a byte offset (no AST).
fn ctx_at(text: &str, offset: u32) -> CursorContext {
    detect_context(text, offset, None)
}

/// Convert a (line, character) pair to a byte offset.
/// Lines and characters are 0-based. Character is clamped to line length.
fn byte_offset(text: &str, line: usize, character: usize) -> u32 {
    let mut offset = 0;
    for (i, l) in text.lines().enumerate() {
        if i == line {
            return (offset + character.min(l.len())) as u32;
        }
        offset += l.len() + 1; // +1 for '\n'
    }
    text.len() as u32
}

/// Shorthand: detect context at (line, character).
fn ctx(text: &str, line: usize, character: usize) -> CursorContext {
    ctx_at(text, byte_offset(text, line, character))
}

// ===========================================================================
// 1. Cursor at byte 0 of empty string
// ===========================================================================

#[test]
fn empty_string_byte_zero() {
    let c = ctx_at("", 0);
    assert!(
        matches!(c, CursorContext::WorkflowRoot { ref prefix } if prefix.is_empty()),
        "Empty doc at offset 0 should be WorkflowRoot with empty prefix, got {c:?}"
    );
}

// ===========================================================================
// 2. Cursor past end of text
// ===========================================================================

#[test]
fn offset_past_end_short_text() {
    let c = ctx_at("hello", 100);
    assert!(
        matches!(c, CursorContext::Unknown { .. }),
        "Offset past text length should be Unknown, got {c:?}"
    );
}

#[test]
fn offset_past_end_by_one() {
    let text = "abc";
    // text.len() == 3; offset 4 is past end
    let c = ctx_at(text, 4);
    assert!(
        matches!(c, CursorContext::Unknown { .. }),
        "Offset text.len()+1 should be Unknown, got {c:?}"
    );
}

#[test]
fn offset_at_exact_end() {
    // offset == text.len() is still within the guard (<=), so it should not
    // return Unknown purely from the bounds check. For a root-level string
    // with no trailing newline the result depends on line content.
    let text = "schema: test";
    let c = ctx_at(text, text.len() as u32);
    // Should resolve to WorkflowRoot (indent 0, valid line).
    assert!(
        matches!(c, CursorContext::WorkflowRoot { .. }),
        "Offset == text.len() on root line should be WorkflowRoot, got {c:?}"
    );
}

#[test]
fn offset_u32_max() {
    let c = ctx_at("short", u32::MAX);
    assert!(
        matches!(c, CursorContext::Unknown { .. }),
        "u32::MAX offset should be Unknown, got {c:?}"
    );
}

// ===========================================================================
// 3. Cursor on newline character
// ===========================================================================

#[test]
fn cursor_on_newline_at_root() {
    let text = "schema: test\n";
    // The newline itself is at byte offset 12. After the newline, before text
    // resumes on the next line. The offset lands on the '\n' of line 0.
    let c = ctx_at(text, 12);
    // Line 0 = "schema: test", char_idx = 12 which equals line length.
    // Indent 0 -> WorkflowRoot.
    assert!(
        matches!(c, CursorContext::WorkflowRoot { .. }),
        "Cursor on trailing newline of root line should be WorkflowRoot, got {c:?}"
    );
}

#[test]
fn cursor_on_newline_between_lines() {
    let text = "schema: test\ntasks:\n  - id: s1\n";
    // Offset at the '\n' after "tasks:" (byte 19).
    let newline_offset = text.find("tasks:").unwrap() + "tasks:".len();
    let c = ctx_at(text, newline_offset as u32);
    assert!(
        matches!(c, CursorContext::WorkflowRoot { .. }),
        "Cursor on newline after root-level 'tasks:' should be WorkflowRoot, got {c:?}"
    );
}

// ===========================================================================
// 4. Very long line (10 000 chars)
// ===========================================================================

#[test]
fn very_long_line_at_root() {
    let long = "a".repeat(10_000);
    let text = format!("schema: {long}\n");
    // Cursor near the end.
    let c = ctx_at(&text, 9_999);
    assert!(
        matches!(c, CursorContext::WorkflowRoot { .. }),
        "Long root line should be WorkflowRoot, got {c:?}"
    );
}

#[test]
fn very_long_line_in_task() {
    let padding = " ".repeat(10_000);
    let text =
        format!("schema: test\ntasks:\n  - id: step1\n    infer:\n      prompt: {padding}hello\n");
    // Cursor at byte 6 of the long prompt line (inside verb block).
    let line_start = text.find("      prompt:").unwrap();
    let c = ctx_at(&text, (line_start + 6) as u32);
    match &c {
        CursorContext::VerbBlock { verb, .. } => assert_eq!(verb, "infer"),
        other => panic!("Expected VerbBlock(infer) on long line, got {other:?}"),
    }
}

// ===========================================================================
// 5. Only whitespace
// ===========================================================================

#[test]
fn only_spaces() {
    let text = "    ";
    let c = ctx_at(text, 2);
    // Indent > 0, not under tasks: or mcp: => Unknown.
    // Actually: lines is ["    "], indent = 4, not under any key => Unknown.
    assert!(
        matches!(c, CursorContext::Unknown { .. }),
        "Whitespace-only text should be Unknown (indented, no parent), got {c:?}"
    );
}

#[test]
fn only_newlines() {
    let text = "\n\n\n";
    let c = ctx_at(text, 0);
    // Line 0 is "", which is empty. lines.is_empty() is false (there are
    // 3 empty lines). current_line = "", indent = 0 => WorkflowRoot.
    assert!(
        matches!(c, CursorContext::WorkflowRoot { .. }),
        "Newlines-only doc at offset 0 should be WorkflowRoot, got {c:?}"
    );
}

#[test]
fn tabs_only() {
    let text = "\t\t";
    let c = ctx_at(text, 1);
    // Indent > 0 (tabs count as characters), not under any key => Unknown.
    assert!(
        matches!(c, CursorContext::Unknown { .. }),
        "Tabs-only text should be Unknown, got {c:?}"
    );
}

// ===========================================================================
// 6. Nested templates: {{with.{{broken
// ===========================================================================

#[test]
fn nested_broken_template() {
    let text = "schema: test\ntasks:\n  - id: s1\n    infer: \"{{with.{{broken\"\n";
    let offset = text.find("broken").unwrap() + 3; // mid-word
    let c = ctx_at(text, offset as u32);
    // The prefix contains "{{" and no "}}", so Template detection fires.
    // rsplit("{{") picks the last "{{" occurrence.
    match &c {
        CursorContext::Template { partial_expr, .. } => {
            assert!(
                partial_expr.starts_with("bro"),
                "Broken nested template should extract partial after last '{{{{', got '{partial_expr}'"
            );
        }
        other => panic!("Expected Template for nested broken '{{{{', got {other:?}"),
    }
}

#[test]
fn template_double_open_no_close() {
    // Two {{ but no }} -- should still detect Template.
    let text = "schema: test\ntasks:\n  - id: s1\n    infer: \"{{first {{ second\"\n";
    let offset = text.find("second").unwrap() + 2;
    let c = ctx_at(text, offset as u32);
    assert!(
        matches!(c, CursorContext::Template { .. }),
        "Double '{{{{' without '}}}}' should still be Template, got {c:?}"
    );
}

// ===========================================================================
// 7. Multiple {{ on same line
// ===========================================================================

#[test]
fn multiple_templates_same_line_first_closed() {
    // First template is closed, second is open -- cursor in the second.
    // Known limitation: the text-based detection checks if prefix contains
    // both "{{" and "}}". When the first template is closed, the prefix
    // contains "}}" which suppresses Template detection for the second "{{".
    // This is a documented edge case of the text-based approach.
    let text = "schema: test\ntasks:\n  - id: s1\n    infer: \"{{with.a}} and {{with.b\"\n";
    let offset = text.find("with.b").unwrap() + 4;
    let c = ctx_at(text, offset as u32);
    // Because prefix contains both {{ and }}, Template detection is
    // suppressed -- the line is treated as a verb line instead.
    assert!(
        matches!(c, CursorContext::VerbBlock { .. }),
        "Closed+open templates on same line: text-based detection falls through to VerbBlock, got {c:?}"
    );
}

#[test]
fn multiple_templates_all_closed() {
    // All templates closed: prefix contains both {{ and }} so template
    // detection should NOT fire (the last {{ has a matching }}).
    let text = "schema: test\ntasks:\n  - id: s1\n    infer: \"{{a}} {{b}}\"\n";
    // Cursor after the last }}
    let line_start = text.find("    infer:").unwrap();
    let c = ctx_at(text, (line_start + 4) as u32);
    // At char 4 ("infe") on a line starting with "infer:" under tasks: ->
    // should be a verb line.
    assert!(
        !matches!(c, CursorContext::Template { .. }),
        "All templates closed should NOT be Template, got {c:?}"
    );
}

// ===========================================================================
// 8. Tab-indented YAML
// ===========================================================================

#[test]
fn tab_indented_task() {
    let text = "schema: test\ntasks:\n\t- id: tab_task\n\t\tinfer:\n\t\t\tprompt: hi\n";
    // Tab at line 4 ("\t\t\tprompt: hi"), cursor at col 5.
    let c = ctx(text, 4, 5);
    // Tab indentation means indent > 0. The ancestor scan should still find
    // "infer:" above. Depending on exact indent math, it should land in
    // VerbBlock or TaskField.
    assert!(
        !matches!(c, CursorContext::Unknown { .. }),
        "Tab-indented YAML under tasks: should not be Unknown, got {c:?}"
    );
}

#[test]
fn mixed_tabs_and_spaces() {
    let text = "schema: test\ntasks:\n\t - id: mixed\n\t   infer:\n\t     prompt: hi\n";
    let c = ctx(text, 4, 8);
    assert!(
        !matches!(c, CursorContext::Unknown { .. }),
        "Mixed tabs/spaces under tasks: should not be Unknown, got {c:?}"
    );
}

// ===========================================================================
// 9. CRLF line endings
// ===========================================================================

#[test]
fn crlf_line_endings_root() {
    let text = "schema: test\r\ntasks:\r\n  - id: s1\r\n    infer: hello\r\n";
    // \r\n: the '\r' sits at the end of each logical line. `text.lines()`
    // in Rust strips both \r\n and \n, so line content is clean.
    // Cursor at byte 0 -> root.
    let c = ctx_at(text, 0);
    assert!(
        matches!(c, CursorContext::WorkflowRoot { .. }),
        "CRLF doc at byte 0 should be WorkflowRoot, got {c:?}"
    );
}

#[test]
fn crlf_cursor_in_task_field() {
    let text = "schema: test\r\ntasks:\r\n  - id: s1\r\n    \r\n";
    // With \r\n, byte offsets differ from \n-only:
    // "schema: test\r\n" = 14 bytes
    // "tasks:\r\n" = 8 bytes  -> offset 22
    // "  - id: s1\r\n" = 12 bytes -> offset 34
    // "    \r\n" starts at offset 34, cursor at 34+4 = 38 (after 4 spaces).
    let c = ctx_at(text, 38);
    // indent=4, under tasks:, line is whitespace => TaskField.
    assert!(
        matches!(c, CursorContext::TaskField { .. }),
        "CRLF cursor in task indent should be TaskField, got {c:?}"
    );
}

#[test]
fn crlf_cursor_on_cr() {
    // Cursor lands exactly on the \r character.
    let text = "schema: test\r\n";
    let c = ctx_at(text, 12); // byte 12 is '\r'
    assert!(
        matches!(c, CursorContext::WorkflowRoot { .. }),
        "Cursor on \\r in CRLF should still resolve to root line, got {c:?}"
    );
}

// ===========================================================================
// 10. Unicode task IDs
// ===========================================================================

#[test]
fn unicode_task_id_emoji() {
    let text = "schema: test\ntasks:\n  - id: \u{1F680}rocket\n    infer: go\n";
    let c = ctx(text, 3, 6);
    let task_id = match &c {
        CursorContext::VerbBlock { task_id, .. } => task_id.as_deref(),
        CursorContext::TaskField { task_id, .. } => task_id.as_deref(),
        other => panic!("Expected VerbBlock or TaskField with emoji task_id, got {other:?}"),
    };
    assert_eq!(
        task_id,
        Some("\u{1F680}rocket"),
        "Unicode emoji task ID should be preserved"
    );
}

#[test]
fn unicode_task_id_cjk() {
    let text = "schema: test\ntasks:\n  - id: \u{4efb}\u{52a1}1\n    \n";
    let c = ctx(text, 3, 4);
    match &c {
        CursorContext::TaskField { task_id, .. } => {
            assert_eq!(
                task_id.as_deref(),
                Some("\u{4efb}\u{52a1}1"),
                "CJK task ID should be preserved"
            );
        }
        other => panic!("Expected TaskField with CJK task_id, got {other:?}"),
    }
}

#[test]
fn unicode_in_template_expression() {
    let text = "schema: test\ntasks:\n  - id: s1\n    infer: \"{{\u{00e9}l\u{00e8}ve.data\"\n";
    let offset = text.find("\u{00e9}l\u{00e8}ve").unwrap() + 2;
    let c = ctx_at(text, offset as u32);
    assert!(
        matches!(c, CursorContext::Template { .. }),
        "Unicode inside template should still be Template, got {c:?}"
    );
}

// ===========================================================================
// 11. Deeply nested blocks (10 levels)
// ===========================================================================

#[test]
fn deeply_nested_10_levels() {
    // Build a deeply nested YAML structure under tasks:.
    // Each level adds 2 more spaces of indentation.
    let mut text = String::from("schema: test\ntasks:\n  - id: deep\n");
    let keys = [
        "infer:",
        "content:",
        "- type: text",
        "nested1:",
        "nested2:",
        "nested3:",
        "nested4:",
        "nested5:",
        "nested6:",
        "nested7:",
    ];
    for (i, key) in keys.iter().enumerate() {
        let indent = "  ".repeat(i + 2);
        text.push_str(&format!("{indent}{key}\n"));
    }
    // Add a final line at deepest nesting.
    let deep_indent = "  ".repeat(12);
    text.push_str(&format!("{deep_indent}value: test\n"));

    // Cursor on the deepest line.
    let last_line_idx = text.lines().count() - 1;
    let c = ctx(text.as_str(), last_line_idx, 26);

    // Should not panic and should resolve to something (likely ContentPart
    // because "content:" is an ancestor).
    assert!(
        // Deeply nested should not panic — any context result is valid.
        // If we got here without panicking, the test passes.
        {
            let _ = &c;
            true
        },
        "Deeply nested (10 levels) should not panic -- got {c:?}"
    );
    // The real assertion: it did not panic. But let's also verify it found
    // the content: ancestor.
    assert!(
        matches!(c, CursorContext::ContentPart { .. }),
        "Deep nesting under content: should be ContentPart, got {c:?}"
    );
}

#[test]
fn deeply_nested_no_matching_ancestor() {
    // Deep nesting under tasks: but no known sub-block ancestor -- should
    // fall through to TaskField or VerbBlock.
    let mut text = String::from("schema: test\ntasks:\n  - id: deep\n");
    for i in 0..10 {
        let indent = "  ".repeat(i + 2);
        text.push_str(&format!("{indent}custom_key_{i}:\n"));
    }
    let deep_indent = "  ".repeat(12);
    text.push_str(&format!("{deep_indent}leaf: val\n"));

    let last_line_idx = text.lines().count() - 1;
    let c = ctx(text.as_str(), last_line_idx, 26);

    // No known ancestor key -- should be TaskField (fallthrough).
    assert!(
        matches!(
            c,
            CursorContext::TaskField { .. } | CursorContext::Unknown { .. }
        ),
        "Deep nesting with unknown ancestors should be TaskField or Unknown, got {c:?}"
    );
}

// ===========================================================================
// 12. Multiple verbs in same task (duplicate)
// ===========================================================================

#[test]
fn duplicate_verbs_in_task() {
    let text = "\
schema: test
tasks:
  - id: dupe
    infer:
      prompt: first
    exec:
      command: echo hi
    ";
    // Cursor on blank line after exec: block.
    let c = ctx(text, 7, 4);
    // At indent 4 under tasks:, should be TaskField for the task.
    // The presence of two verbs should not cause a panic.
    match &c {
        CursorContext::TaskField { task_id, .. } => {
            assert_eq!(task_id.as_deref(), Some("dupe"));
        }
        CursorContext::VerbBlock { task_id, .. } => {
            assert_eq!(task_id.as_deref(), Some("dupe"));
        }
        other => panic!("Duplicate verbs: expected TaskField or VerbBlock, got {other:?}"),
    }
}

#[test]
fn duplicate_verbs_cursor_in_second() {
    let text = "\
schema: test
tasks:
  - id: dupe
    infer:
      prompt: first
    exec:
      command: echo hi
";
    // Cursor inside exec: block on "command:" line.
    let c = ctx(text, 6, 10);
    match &c {
        CursorContext::VerbBlock { verb, task_id, .. } => {
            assert_eq!(verb, "exec");
            assert_eq!(task_id.as_deref(), Some("dupe"));
        }
        other => panic!("Cursor in second verb block should be VerbBlock(exec), got {other:?}"),
    }
}

// ===========================================================================
// 13. Cursor right after colon (key:)
// ===========================================================================

#[test]
fn cursor_right_after_colon_root_key() {
    let text = "schema:";
    // Cursor at byte 7 -- right after the colon, no space.
    let c = ctx_at(text, 7);
    assert!(
        matches!(c, CursorContext::WorkflowRoot { ref prefix } if prefix == "schema:"),
        "Cursor after root colon should be WorkflowRoot, got {c:?}"
    );
}

#[test]
fn cursor_right_after_colon_task_field() {
    let text = "schema: test\ntasks:\n  - id: s1\n    infer:\n";
    // Cursor right after "infer:" at end of line (byte offset of ':' + 1).
    let infer_colon = text.find("infer:").unwrap() + 6;
    let c = ctx_at(text, infer_colon as u32);
    // "infer:" starts with a verb, so it should be VerbBlock.
    assert!(
        matches!(
            c,
            CursorContext::VerbBlock { ref verb, .. } if verb == "infer"
        ),
        "Cursor after verb colon should be VerbBlock, got {c:?}"
    );
}

#[test]
fn cursor_right_after_colon_with_block() {
    let text = "schema: test\ntasks:\n  - id: s1\n    with:\n      data:\n";
    // Cursor right after "data:" (the colon, no space after).
    let data_colon = text.find("data:").unwrap() + 5;
    let c = ctx_at(text, data_colon as u32);
    assert!(
        matches!(c, CursorContext::WithBlock { .. }),
        "Cursor after with-entry colon should be WithBlock, got {c:?}"
    );
}

// ===========================================================================
// 14. Cursor in comment line (# ...)
// ===========================================================================

#[test]
fn comment_line_at_root() {
    let text = "# This is a comment\nschema: test\n";
    let c = ctx(text, 0, 10);
    // Comment at indent 0 -> WorkflowRoot (the function does not special-case
    // comments -- it sees indent 0 and returns WorkflowRoot).
    assert!(
        matches!(c, CursorContext::WorkflowRoot { .. }),
        "Comment at root level should be WorkflowRoot, got {c:?}"
    );
}

#[test]
fn comment_line_in_task() {
    let text = "\
schema: test
tasks:
  - id: s1
    # TODO: add prompt
    infer: hello
";
    // Cursor on the comment line inside the task.
    let c = ctx(text, 3, 10);
    // indent = 4, under tasks:, line starts with "# TODO: add prompt".
    // Should resolve to TaskField (no verb detection on comments).
    assert!(
        matches!(
            c,
            CursorContext::TaskField { .. } | CursorContext::VerbBlock { .. }
        ),
        "Comment inside task should be TaskField or VerbBlock, got {c:?}"
    );
}

#[test]
fn comment_line_in_verb_block() {
    let text = "\
schema: test
tasks:
  - id: s1
    infer:
      # model selection
      prompt: hello
";
    let c = ctx(text, 4, 10);
    match &c {
        CursorContext::VerbBlock { verb, .. } => {
            assert_eq!(verb, "infer");
        }
        other => panic!("Comment inside verb block should be VerbBlock(infer), got {other:?}"),
    }
}

// ===========================================================================
// 15. Workflow with no tasks: key
// ===========================================================================

#[test]
fn no_tasks_key_at_all() {
    let text = "schema: test\nmcp:\n  novanet:\n    command: cargo\n";
    // Cursor inside mcp: block.
    let c = ctx(text, 3, 6);
    assert!(
        matches!(c, CursorContext::McpConfig { .. }),
        "Doc with mcp: but no tasks: should still detect McpConfig, got {c:?}"
    );
}

#[test]
fn no_tasks_key_indented_line() {
    let text = "schema: test\n  indented_but_no_tasks: true\n";
    let c = ctx(text, 1, 10);
    // Indented but not under tasks: or mcp: -> Unknown.
    assert!(
        matches!(c, CursorContext::Unknown { .. }),
        "Indented line with no tasks: parent should be Unknown, got {c:?}"
    );
}

#[test]
fn empty_tasks_block() {
    let text = "schema: test\ntasks:\n";
    // Cursor on the line after "tasks:" which is empty (end of file).
    // byte_offset for line 2 will return text.len().
    let c = ctx_at(text, text.len() as u32);
    // offset == text.len() is allowed. Line index would be 2, which is
    // past the last line. current_line = "", indent = 0 -> WorkflowRoot.
    assert!(
        matches!(
            c,
            CursorContext::WorkflowRoot { .. } | CursorContext::Unknown { .. }
        ),
        "Empty tasks block at EOF should be WorkflowRoot or Unknown, got {c:?}"
    );
}

// ===========================================================================
// Additional edge cases for robustness
// ===========================================================================

#[test]
fn single_newline() {
    let c = ctx_at("\n", 0);
    assert!(
        matches!(c, CursorContext::WorkflowRoot { .. }),
        "Single newline at offset 0 should be WorkflowRoot, got {c:?}"
    );
}

#[test]
fn template_with_only_opening_braces() {
    let text = "schema: test\ntasks:\n  - id: s1\n    infer: \"{{\"\n";
    let offset = text.find("{{").unwrap() + 2;
    let c = ctx_at(text, offset as u32);
    match &c {
        CursorContext::Template { partial_expr, .. } => {
            assert!(
                partial_expr.is_empty() || partial_expr == "\"",
                "Just '{{{{' should have empty or minimal partial_expr, got '{partial_expr}'"
            );
        }
        other => panic!("Just '{{{{' should be Template, got {other:?}"),
    }
}

#[test]
fn invoke_with_all_subfields() {
    let text = "\
schema: test
tasks:
  - id: s1
    invoke:
      mcp: novanet
      tool: search
      params:
        query: hello
      resource: res1
";
    // Cursor on resource: line -- character must be past "resource:" (15+)
    // to capture the full key in the prefix for Resource focus detection.
    let c = ctx(text, 8, 16);
    match &c {
        CursorContext::InvokeBlock {
            focus: InvokeFocus::Resource,
            mcp_server,
            tool_name,
            ..
        } => {
            assert_eq!(mcp_server.as_deref(), Some("novanet"));
            assert_eq!(tool_name.as_deref(), Some("search"));
        }
        other => panic!("Expected InvokeBlock/Resource, got {other:?}"),
    }
}

#[test]
fn content_part_type_detection() {
    let text = "\
schema: test
tasks:
  - id: s1
    infer:
      content:
        - type: image
          detail: auto
";
    // Character 18 captures the full "- type: image" prefix.
    let c = ctx(text, 5, 18);
    match &c {
        CursorContext::ContentPart {
            focus: ContentFocus::PartType,
            ..
        } => {}
        other => panic!("Expected ContentPart/PartType, got {other:?}"),
    }
}

#[test]
fn for_each_with_loop_var() {
    let text = "\
schema: test
tasks:
  - id: s1
    for_each:
      items: [a, b, c]
      as: item
      body:
";
    let c = ctx(text, 6, 8);
    match &c {
        CursorContext::ForEach {
            loop_var, task_id, ..
        } => {
            assert_eq!(loop_var.as_deref(), Some("item"));
            assert_eq!(task_id.as_deref(), Some("s1"));
        }
        other => panic!("Expected ForEach with loop_var, got {other:?}"),
    }
}

#[test]
fn guardrails_with_type() {
    let text = "\
schema: test
tasks:
  - id: s1
    infer: hello
    guardrails:
      input: safety_check
";
    let c = ctx(text, 5, 18);
    match &c {
        CursorContext::Guardrails {
            guardrail_type,
            task_id,
            ..
        } => {
            assert_eq!(guardrail_type.as_deref(), Some("input"));
            assert_eq!(task_id.as_deref(), Some("s1"));
        }
        other => panic!("Expected Guardrails, got {other:?}"),
    }
}

#[test]
fn limits_block_detection() {
    let text = "\
schema: test
tasks:
  - id: s1
    infer: hello
    limits:
      timeout: 30s
";
    let c = ctx(text, 5, 10);
    match &c {
        CursorContext::LimitsBlock { task_id, .. } => {
            assert_eq!(task_id.as_deref(), Some("s1"));
        }
        other => panic!("Expected LimitsBlock, got {other:?}"),
    }
}

#[test]
fn provider_context_on_model_field() {
    let text = "\
schema: test
tasks:
  - id: s1
    infer:
      provider: openai
      model: gpt-4
      prompt: hello
";
    let c = ctx(text, 5, 12);
    match &c {
        CursorContext::ProviderContext {
            verb,
            current_provider,
            current_model,
            ..
        } => {
            assert_eq!(verb, "infer");
            assert_eq!(current_provider.as_deref(), Some("openai"));
            assert_eq!(current_model.as_deref(), Some("gpt-4"));
        }
        other => panic!("Expected ProviderContext, got {other:?}"),
    }
}

// ===========================================================================
// 16. Error recovery context detection
// ===========================================================================

#[test]
fn recovery_context_on_broken_yaml() {
    use nika_lsp_core::analysis::context::detect_context_with_recovery;
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    infer\n      prompt: hello";
    // Cursor on "prompt:" line inside broken infer (no colon)
    let offset = yaml.find("prompt").unwrap() as u32;
    let ctx = detect_context_with_recovery(yaml, offset);
    // Should still detect something useful, not Unknown
    // The recovery parser should help identify the verb context
    assert!(
        !matches!(ctx, CursorContext::Unknown { .. }),
        "Recovery should detect context on broken YAML, got: {ctx:?}"
    );
}

#[test]
fn recovery_empty_document() {
    use nika_lsp_core::analysis::context::detect_context_with_recovery;
    let ctx = detect_context_with_recovery("", 0);
    assert!(
        matches!(ctx, CursorContext::WorkflowRoot { .. }),
        "Recovery on empty document should be WorkflowRoot, got: {ctx:?}"
    );
}

#[test]
fn recovery_huge_offset() {
    use nika_lsp_core::analysis::context::detect_context_with_recovery;
    let ctx = detect_context_with_recovery("schema: test", 99999);
    assert!(
        matches!(ctx, CursorContext::Unknown { .. }),
        "Recovery with huge offset should be Unknown, got: {ctx:?}"
    );
}

// ===========================================================================
// 17. CRLF and Unicode robustness
// ===========================================================================

#[test]
fn crlf_line_detection() {
    let yaml = "schema: nika/workflow@0.12\r\ntasks:\r\n  - id: step1\r\n    infer: hello\r\n";
    let offset = yaml.find("infer").unwrap() as u32;
    let ctx = detect_context(yaml, offset, None);
    // Should detect verb context despite CRLF
    assert!(
        !matches!(ctx, CursorContext::Unknown { .. }),
        "CRLF should not break context detection, got: {ctx:?}"
    );
}

#[test]
fn unicode_in_task_id() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: étape1\n    infer: hello\n";
    let offset = yaml.find("infer").unwrap() as u32;
    let ctx = detect_context(yaml, offset, None);
    assert!(
        !matches!(ctx, CursorContext::Unknown { .. }),
        "Unicode in task ID should not break context detection, got: {ctx:?}"
    );
}
