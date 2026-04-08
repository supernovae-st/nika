// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! End-to-end tests for the references handler.
//!
//! Each test builds a realistic Nika workflow YAML and exercises
//! `find_task_at_offset` and `find_task_references` through the public API.

use nika_lsp_core::handlers::references::{find_task_at_offset, find_task_references};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find the byte offset of `needle` in `text`, panicking if not found.
fn offset_of(text: &str, needle: &str) -> u32 {
    text.find(needle)
        .unwrap_or_else(|| panic!("needle {needle:?} not found in text")) as u32
}

/// Find the byte offset of the Nth occurrence of `needle` in `text` (1-based).
fn offset_of_nth(text: &str, needle: &str, nth: usize) -> u32 {
    let mut start = 0;
    for _ in 1..nth {
        let pos = text[start..]
            .find(needle)
            .unwrap_or_else(|| panic!("needle {needle:?} occurrence {nth} not found"));
        start += pos + needle.len();
    }
    let pos = text[start..]
        .find(needle)
        .unwrap_or_else(|| panic!("needle {needle:?} occurrence {nth} not found"));
    (start + pos) as u32
}

/// Assert that a reference entry points exactly at `expected` in the text.
fn assert_ref_text(
    text: &str,
    entry: &nika_lsp_core::handlers::references::ReferenceEntry,
    expected: &str,
) {
    let slice = &text[entry.start_offset as usize..entry.end_offset as usize];
    assert_eq!(
        slice, expected,
        "expected reference to point at {expected:?}, got {slice:?}"
    );
}

// ---------------------------------------------------------------------------
// Workflow fixtures
// ---------------------------------------------------------------------------

const SIMPLE_WORKFLOW: &str = "\
schema: \"@0.12\"
workflow: simple
provider: anthropic

tasks:
  - id: step1
    infer: \"Generate a greeting\"

  - id: step2
    depends_on: [step1]
    infer: \"Process the greeting\"
";

const WITH_BINDING_WORKFLOW: &str = "\
schema: \"@0.12\"
workflow: bindings
provider: anthropic

tasks:
  - id: fetch_data
    exec: \"curl http://example.com\"

  - id: transform
    with:
      raw: $fetch_data
    infer: \"Transform {{with.raw}}\"
    depends_on: [fetch_data]
";

const MULTILINE_DEPS_WORKFLOW: &str = "\
schema: \"@0.12\"
workflow: multiline
provider: anthropic

tasks:
  - id: alpha
    infer: \"A\"

  - id: beta
    infer: \"B\"

  - id: gamma
    depends_on:
      - alpha
      - beta
    infer: \"C\"
";

const SCALAR_DEPS_WORKFLOW: &str = "\
schema: \"@0.12\"
workflow: scalar
provider: anthropic

tasks:
  - id: first
    infer: \"A\"

  - id: second
    depends_on: first
    infer: \"B\"
";

// ===========================================================================
// find_task_at_offset tests
// ===========================================================================

#[test]
fn find_task_at_cursor_on_id_line() {
    let offset = offset_of(SIMPLE_WORKFLOW, "step1");
    let result = find_task_at_offset(SIMPLE_WORKFLOW, offset);
    assert_eq!(result, Some("step1".to_string()));
}

#[test]
fn find_task_at_cursor_on_depends_on_inline() {
    // The second occurrence of "step1" is inside depends_on: [step1]
    let offset = offset_of_nth(SIMPLE_WORKFLOW, "step1", 2);
    let result = find_task_at_offset(SIMPLE_WORKFLOW, offset);
    assert_eq!(result, Some("step1".to_string()));
}

#[test]
fn find_task_at_cursor_on_with_dollar_ref() {
    let offset = offset_of(WITH_BINDING_WORKFLOW, "$fetch_data");
    let result = find_task_at_offset(WITH_BINDING_WORKFLOW, offset);
    assert_eq!(result, Some("fetch_data".to_string()));
}

#[test]
fn find_task_at_cursor_on_depends_on_scalar() {
    // In SCALAR_DEPS_WORKFLOW, "first" appears in `- id: first` and `depends_on: first`
    let offset = offset_of_nth(SCALAR_DEPS_WORKFLOW, "first", 2);
    let result = find_task_at_offset(SCALAR_DEPS_WORKFLOW, offset);
    assert_eq!(result, Some("first".to_string()));
}

#[test]
fn find_task_at_cursor_on_multiline_dep_item() {
    // In MULTILINE_DEPS_WORKFLOW, find "alpha" in the `- alpha` list item (not the id line)
    let offset = offset_of_nth(MULTILINE_DEPS_WORKFLOW, "alpha", 2);
    let result = find_task_at_offset(MULTILINE_DEPS_WORKFLOW, offset);
    assert_eq!(result, Some("alpha".to_string()));
}

#[test]
fn find_task_at_cursor_not_on_any_task() {
    // Cursor on "schema" line
    let result = find_task_at_offset(SIMPLE_WORKFLOW, 0);
    assert!(result.is_none());
}

#[test]
fn find_task_at_cursor_empty_document() {
    let result = find_task_at_offset("", 0);
    assert!(result.is_none());
}

// ===========================================================================
// find_task_references tests
// ===========================================================================

#[test]
fn references_returns_definition_plus_depends_on() {
    let refs = find_task_references(SIMPLE_WORKFLOW, "step1");
    assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
    assert_ref_text(SIMPLE_WORKFLOW, &refs[0], "step1");
    assert_ref_text(SIMPLE_WORKFLOW, &refs[1], "step1");
    // First is definition, second is in depends_on
    assert_ne!(refs[0].start_offset, refs[1].start_offset);
}

#[test]
fn references_returns_definition_plus_deps_plus_bindings() {
    let refs = find_task_references(WITH_BINDING_WORKFLOW, "fetch_data");
    // definition + with: $fetch_data + depends_on: [fetch_data] + template alias (raw)
    assert!(
        refs.len() >= 3,
        "Expected at least 3 references, got {}: {:?}",
        refs.len(),
        refs
    );
    for r in &refs {
        let slice = &WITH_BINDING_WORKFLOW[r.start_offset as usize..r.end_offset as usize];
        assert!(
            slice == "fetch_data" || slice == "raw",
            "expected 'fetch_data' or 'raw', got '{slice}'"
        );
    }
}

#[test]
fn references_multiline_depends_on() {
    let refs = find_task_references(MULTILINE_DEPS_WORKFLOW, "alpha");
    // definition (- id: alpha) + multiline dep (- alpha)
    assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
    assert_ref_text(MULTILINE_DEPS_WORKFLOW, &refs[0], "alpha");
    assert_ref_text(MULTILINE_DEPS_WORKFLOW, &refs[1], "alpha");
}

#[test]
fn references_scalar_depends_on() {
    let refs = find_task_references(SCALAR_DEPS_WORKFLOW, "first");
    assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
    assert_ref_text(SCALAR_DEPS_WORKFLOW, &refs[0], "first");
    assert_ref_text(SCALAR_DEPS_WORKFLOW, &refs[1], "first");
}

#[test]
fn references_no_references_for_unknown_task() {
    let refs = find_task_references(SIMPLE_WORKFLOW, "nonexistent");
    assert!(refs.is_empty());
}

#[test]
fn references_definition_only_for_isolated_task() {
    let refs = find_task_references(SIMPLE_WORKFLOW, "step2");
    // step2 has no references FROM other tasks; it only has its own definition
    assert_eq!(
        refs.len(),
        1,
        "Expected 1 reference (def only), got: {:?}",
        refs
    );
    assert_ref_text(SIMPLE_WORKFLOW, &refs[0], "step2");
}

#[test]
fn references_template_alias_included() {
    let text = "\
tasks:
  - id: step1
    infer: \"Hello\"

  - id: step2
    with:
      result: $step1
    infer: \"Process {{with.result}}\"
";
    let refs = find_task_references(text, "step1");
    // definition + with ref + template alias
    assert_eq!(refs.len(), 3, "Expected 3 references, got: {:?}", refs);
    assert_ref_text(text, &refs[0], "step1");
    assert_ref_text(text, &refs[1], "step1");
    assert_ref_text(text, &refs[2], "result"); // template alias
}

#[test]
fn references_template_dollar_included() {
    let text = "\
tasks:
  - id: data
    infer: \"Get data\"

  - id: process
    infer: \"Process {{$data}}\"
";
    let refs = find_task_references(text, "data");
    // definition + template {{$data}}
    assert_eq!(refs.len(), 2, "Expected 2 references, got: {:?}", refs);
    assert_ref_text(text, &refs[0], "data");
    assert_ref_text(text, &refs[1], "data");
}

#[test]
fn references_byte_offsets_are_exact() {
    let text = "tasks:\n  - id: step1\n    depends_on: [step1]";
    let refs = find_task_references(text, "step1");
    assert_eq!(refs.len(), 2);

    for r in &refs {
        let slice = &text[r.start_offset as usize..r.end_offset as usize];
        assert_eq!(
            slice, "step1",
            "Byte offset should point exactly at 'step1'"
        );
        assert_eq!(
            (r.end_offset - r.start_offset) as usize,
            "step1".len(),
            "Length should match task ID length"
        );
    }
}

// ===========================================================================
// Handler trait integration test
// ===========================================================================

#[test]
fn handler_trait_references() {
    use nika_lsp_core::handler::{DefaultHandler, LspHandler};

    let h = DefaultHandler::new();

    // find_task_at_offset via trait
    let offset = offset_of(SIMPLE_WORKFLOW, "step1");
    let task = h.find_task_at_offset(SIMPLE_WORKFLOW, offset);
    assert_eq!(task, Some("step1".to_string()));

    // references via trait
    let refs = h.references(SIMPLE_WORKFLOW, "step1");
    assert_eq!(refs.len(), 2);
}
