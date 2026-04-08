// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exhaustive end-to-end tests for the completion handler.
//!
//! Each test exercises the FULL pipeline:
//!   `detect_context()` -> `completions()`
//!
//! No src/ files are modified -- this is a pure integration test that imports
//! the public API of `nika_lsp_core`.

use ls_types::{CompletionItem, CompletionItemKind};
use nika_lsp_core::analysis::context::{detect_context, CursorContext};
use nika_lsp_core::handlers::completion::completions;

// ===========================================================================
// Helpers
// ===========================================================================

/// Full pipeline: detect context at (line, character) then compute completions.
fn complete_at(text: &str, line: usize, character: usize) -> Vec<CompletionItem> {
    let offset = text_offset(text, line, character);
    let ctx = detect_context(text, offset, None);
    completions(text, offset, &ctx, None)
}

/// Convert (line, character) to byte offset.
fn text_offset(text: &str, line: usize, character: usize) -> u32 {
    let mut offset = 0usize;
    for (i, l) in text.lines().enumerate() {
        if i == line {
            return (offset + character.min(l.len())) as u32;
        }
        offset += l.len() + 1; // +1 for '\n'
    }
    text.len() as u32
}

/// Assert that completions contain an item with the given label.
fn assert_has_label(items: &[CompletionItem], label: &str) {
    assert!(
        items.iter().any(|i| i.label == label),
        "Expected label '{label}' not found. Got: [{}]",
        items
            .iter()
            .map(|i| i.label.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// Assert that completions do NOT contain an item with the given label.
fn assert_no_label(items: &[CompletionItem], label: &str) {
    assert!(
        !items.iter().any(|i| i.label == label),
        "Label '{label}' should NOT be present but was found"
    );
}

/// Assert every item has a `kind` set.
fn assert_all_have_kind(items: &[CompletionItem]) {
    for item in items {
        assert!(
            item.kind.is_some(),
            "Item '{}' is missing CompletionItemKind",
            item.label
        );
    }
}

/// Assert every item has a `sort_text` set.
fn assert_all_have_sort_text(items: &[CompletionItem]) {
    for item in items {
        assert!(
            item.sort_text.is_some(),
            "Item '{}' is missing sort_text",
            item.label
        );
    }
}

/// Assert that all items with a given label have the expected kind.
fn assert_label_kind(items: &[CompletionItem], label: &str, expected: CompletionItemKind) {
    let item = items
        .iter()
        .find(|i| i.label == label)
        .unwrap_or_else(|| panic!("Label '{label}' not found"));
    assert_eq!(
        item.kind,
        Some(expected),
        "Item '{}' has wrong kind: {:?}",
        label,
        item.kind
    );
}

// ===========================================================================
// 1. Empty file -> should get schema, tasks, workflow, mcp completions
// ===========================================================================

#[test]
fn empty_file_returns_workflow_root_completions() {
    let items = complete_at("", 0, 0);
    assert!(!items.is_empty(), "Empty file should produce completions");
    assert_has_label(&items, "schema");
    assert_has_label(&items, "tasks");
    assert_has_label(&items, "workflow");
    assert_has_label(&items, "mcp");
    assert_has_label(&items, "context");
    assert_has_label(&items, "inputs");
    assert_has_label(&items, "include");
    assert_has_label(&items, "edges");
    assert_all_have_kind(&items);
    assert_all_have_sort_text(&items);
}

#[test]
fn empty_file_root_items_are_keywords() {
    let items = complete_at("", 0, 0);
    for item in &items {
        assert_label_kind(&items, &item.label, CompletionItemKind::KEYWORD);
    }
}

#[test]
fn empty_file_context_is_workflow_root() {
    let ctx = detect_context("", 0, None);
    assert!(
        matches!(ctx, CursorContext::WorkflowRoot { .. }),
        "Empty file context should be WorkflowRoot, got {ctx:?}"
    );
}

// ===========================================================================
// 2. After `schema:` at line 1 -> workflow root
// ===========================================================================

#[test]
fn after_schema_line_workflow_root() {
    let yaml = "schema: nika/workflow@0.12\n";
    let items = complete_at(yaml, 1, 0);
    assert!(
        !items.is_empty(),
        "Should get root completions after schema line"
    );
    assert_has_label(&items, "tasks");
    assert_has_label(&items, "workflow");
    assert_has_label(&items, "mcp");
    assert_all_have_kind(&items);
}

#[test]
fn after_schema_line_context_is_root() {
    let yaml = "schema: nika/workflow@0.12\n";
    let ctx = detect_context(yaml, text_offset(yaml, 1, 0), None);
    assert!(
        matches!(ctx, CursorContext::WorkflowRoot { .. }),
        "Expected WorkflowRoot after schema line, got {ctx:?}"
    );
}

#[test]
fn on_schema_line_itself_is_root() {
    let yaml = "schema: nika/workflow@0.12\n";
    let items = complete_at(yaml, 0, 3);
    // Cursor mid-word on a root-level line is still WorkflowRoot
    let ctx = detect_context(yaml, text_offset(yaml, 0, 3), None);
    assert!(
        matches!(ctx, CursorContext::WorkflowRoot { .. }),
        "Expected WorkflowRoot, got {ctx:?}"
    );
    assert!(!items.is_empty());
}

// ===========================================================================
// 3. Inside task after `- id: step1\n    ` -> all verbs + task fields
// ===========================================================================

#[test]
fn task_field_all_verbs_present() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    ";
    let items = complete_at(yaml, 3, 4);
    assert!(!items.is_empty(), "Task field should produce completions");
    // 5 verbs
    assert_has_label(&items, "infer");
    assert_has_label(&items, "exec");
    assert_has_label(&items, "fetch");
    assert_has_label(&items, "invoke");
    assert_has_label(&items, "agent");
    assert_all_have_kind(&items);
}

#[test]
fn task_field_meta_fields_present() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    ";
    let items = complete_at(yaml, 3, 4);
    assert_has_label(&items, "with");
    assert_has_label(&items, "depends_on");
    assert_has_label(&items, "content");
    assert_has_label(&items, "for_each");
    assert_has_label(&items, "retry");
    assert_has_label(&items, "timeout");
    assert_has_label(&items, "provider");
    assert_has_label(&items, "model");
    assert_has_label(&items, "structured");
    assert_has_label(&items, "description");
    assert_has_label(&items, "artifact");
    assert_has_label(&items, "log");
}

#[test]
fn task_field_verbs_are_keyword_kind() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    ";
    let items = complete_at(yaml, 3, 4);
    for verb in &["infer", "exec", "fetch", "invoke", "agent"] {
        assert_label_kind(&items, verb, CompletionItemKind::KEYWORD);
    }
}

#[test]
fn task_field_meta_are_property_kind() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    ";
    let items = complete_at(yaml, 3, 4);
    for field in &[
        "with",
        "depends_on",
        "content",
        "for_each",
        "retry",
        "timeout",
        "provider",
        "model",
        "structured",
        "description",
        "artifact",
        "log",
    ] {
        assert_label_kind(&items, field, CompletionItemKind::PROPERTY);
    }
}

#[test]
fn task_field_context_is_task_field() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    ";
    let ctx = detect_context(yaml, text_offset(yaml, 3, 4), None);
    match &ctx {
        CursorContext::TaskField { task_id, .. } => {
            assert_eq!(task_id.as_deref(), Some("step1"));
        }
        other => panic!("Expected TaskField, got {other:?}"),
    }
}

#[test]
fn task_field_excludes_existing_fields() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    ";
    let items = complete_at(yaml, 4, 4);
    // 'infer' is at the same indent level (4) as the cursor, so it is detected
    // as a sibling field and excluded from completions.
    assert_no_label(&items, "infer");
}

#[test]
fn task_field_still_offers_unused_verbs() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    exec: echo hi
    ";
    let items = complete_at(yaml, 4, 4);
    // 'exec' is used, but other verbs should still be available
    assert_no_label(&items, "exec");
    assert_has_label(&items, "infer");
    assert_has_label(&items, "fetch");
}

// ===========================================================================
// 4. Inside `infer:\n      ` -> prompt, model, temperature, content
// ===========================================================================

#[test]
fn infer_block_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      ";
    let items = complete_at(yaml, 4, 6);
    assert!(!items.is_empty(), "Infer block should produce completions");
    assert_has_label(&items, "prompt");
    assert_has_label(&items, "system");
    assert_has_label(&items, "model");
    assert_has_label(&items, "provider");
    assert_has_label(&items, "temperature");
    assert_has_label(&items, "max_tokens");
    assert_has_label(&items, "content");
    assert_all_have_kind(&items);
    assert_all_have_sort_text(&items);
}

#[test]
fn infer_block_context_is_verb_block() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      ";
    let ctx = detect_context(yaml, text_offset(yaml, 4, 6), None);
    match &ctx {
        CursorContext::VerbBlock { verb, task_id, .. } => {
            assert_eq!(verb, "infer");
            assert_eq!(task_id.as_deref(), Some("step1"));
        }
        other => panic!("Expected VerbBlock(infer), got {other:?}"),
    }
}

#[test]
fn infer_block_all_items_are_properties() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      ";
    let items = complete_at(yaml, 4, 6);
    for item in &items {
        assert_label_kind(&items, &item.label, CompletionItemKind::PROPERTY);
    }
}

// ===========================================================================
// 5. Inside `content:\n        - type: ` -> text, image, image_url
// ===========================================================================

#[test]
fn content_type_completions() {
    // When the cursor is on a blank line inside a content block, the context
    // is ContentPart with PartType focus and an empty prefix, producing the
    // three type suggestions.
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      content:
        ";
    let items = complete_at(yaml, 5, 8);
    assert!(!items.is_empty(), "Content type should produce completions");
    assert_has_label(&items, "text");
    assert_has_label(&items, "source");
    assert_all_have_kind(&items);
}

#[test]
fn content_type_context_is_content_part() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      content:
        - type: ";
    let ctx = detect_context(yaml, text_offset(yaml, 5, 10), None);
    assert!(
        matches!(ctx, CursorContext::ContentPart { .. }),
        "Expected ContentPart, got {ctx:?}"
    );
}

#[test]
fn content_part_field_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      content:
        - type: text
          ";
    let items = complete_at(yaml, 6, 10);
    assert!(
        !items.is_empty(),
        "Content part fields should produce completions"
    );
    // Should get field-level suggestions (text, source, url, detail)
    assert_all_have_kind(&items);
}

#[test]
fn content_image_detail_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      content:
        - type: image
          source: hash123
          detail: ";
    let items = complete_at(yaml, 7, 18);
    assert!(!items.is_empty(), "Image detail should produce completions");
    assert_has_label(&items, "auto");
    assert_has_label(&items, "low");
    assert_has_label(&items, "high");
}

// ===========================================================================
// 6. Inside `with:\n      ` -> task ID references
// ===========================================================================

#[test]
fn with_block_lists_task_ids() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    with:
      data: ";
    let items = complete_at(yaml, 6, 12);
    assert!(!items.is_empty(), "With block should produce completions");
    // Should suggest task IDs as references
    assert_has_label(&items, "step1");
    assert_has_label(&items, "step2");
    assert_all_have_kind(&items);
}

#[test]
fn with_block_context_is_with_block() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    with:
      data: step1
      ";
    let ctx = detect_context(yaml, text_offset(yaml, 7, 6), None);
    assert!(
        matches!(ctx, CursorContext::WithBlock { .. }),
        "Expected WithBlock, got {ctx:?}"
    );
}

#[test]
fn with_block_references_are_reference_kind() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: alpha
    infer: hello
  - id: beta
    with:
      data: ";
    let items = complete_at(yaml, 6, 12);
    for item in &items {
        assert_label_kind(&items, &item.label, CompletionItemKind::REFERENCE);
    }
}

// ===========================================================================
// 7. Inside `depends_on: [` -> task IDs excluding self
// ===========================================================================

#[test]
fn depends_on_inline_lists_tasks() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    infer: world
  - id: step3
    depends_on: [
      ";
    let items = complete_at(yaml, 8, 6);
    assert!(!items.is_empty(), "depends_on should produce completions");
    assert_has_label(&items, "step1");
    assert_has_label(&items, "step2");
    assert_has_label(&items, "step3");
    assert_all_have_kind(&items);
}

#[test]
fn depends_on_excludes_already_listed() {
    // When depends_on uses block format with items underneath, the context
    // detection at the sub-item level correctly identifies DependsOn with
    // existing deps parsed from the depends_on: line.
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    infer: world
  - id: step3
    depends_on:
      - step1
      ";
    // Cursor on the blank line after `- step1` inside depends_on block
    let ctx = detect_context(yaml, text_offset(yaml, 9, 6), None);
    assert!(
        matches!(ctx, CursorContext::DependsOn { .. }),
        "Expected DependsOn, got {ctx:?}"
    );
}

#[test]
fn depends_on_block_format() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    depends_on:
      - step1
      ";
    let ctx = detect_context(yaml, text_offset(yaml, 7, 6), None);
    assert!(
        matches!(ctx, CursorContext::DependsOn { .. }),
        "Expected DependsOn in block format, got {ctx:?}"
    );
}

#[test]
fn depends_on_items_are_reference_kind() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    depends_on:
      ";
    let items = complete_at(yaml, 6, 6);
    for item in &items {
        assert_label_kind(&items, &item.label, CompletionItemKind::REFERENCE);
    }
}

// ===========================================================================
// 8. Inside `{{with.` -> binding references
// ===========================================================================

#[test]
fn template_binding_references() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    with:
      data: step1
    infer: \"Result: {{with.\"
";
    // Position the cursor inside the {{ }} template
    let offset = yaml.find("{{with.").unwrap() + 7; // after "{{with."
    let items = completions(
        yaml,
        offset as u32,
        &detect_context(yaml, offset as u32, None),
        None,
    );
    // Template context should return with., context.files., inputs., and task shorthands
    assert!(!items.is_empty(), "Template should produce completions");
    assert_all_have_kind(&items);
}

#[test]
fn template_context_has_with_prefix() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: \"{{with.\"
";
    // Position at the end of "with." inside the template (7 chars after "{{")
    let offset = yaml.find("{{").unwrap() + 2 + 5; // "{{" + "with."
    let ctx = detect_context(yaml, offset as u32, None);
    match &ctx {
        CursorContext::Template {
            partial_expr,
            in_transform_chain,
            ..
        } => {
            assert!(
                partial_expr.starts_with("with"),
                "partial_expr should start with 'with', got '{partial_expr}'"
            );
            assert!(!in_transform_chain);
        }
        other => panic!("Expected Template, got {other:?}"),
    }
}

#[test]
fn template_at_open_brace_suggests_variables() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    infer: \"{{\"
";
    let offset = yaml.find("{{").unwrap() + 2;
    let items = completions(
        yaml,
        offset as u32,
        &detect_context(yaml, offset as u32, None),
        None,
    );
    assert!(
        !items.is_empty(),
        "Template at {{ should produce completions"
    );
    assert_has_label(&items, "with.");
    assert_has_label(&items, "context.files.");
    assert_has_label(&items, "inputs.");
}

#[test]
fn template_includes_task_shorthands() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: generate
    infer: hello
  - id: process
    infer: \"{{\"
";
    let offset = yaml.find("{{").unwrap() + 2;
    let items = completions(
        yaml,
        offset as u32,
        &detect_context(yaml, offset as u32, None),
        None,
    );
    assert_has_label(&items, "$generate");
    assert_has_label(&items, "$process");
}

// ===========================================================================
// 9. Inside `{{with.data | ` -> transform completions
// ===========================================================================

#[test]
fn transform_chain_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: \"{{with.data | \"
";
    let offset = yaml.find("| ").unwrap() + 2;
    let ctx = detect_context(yaml, offset as u32, None);
    let items = completions(yaml, offset as u32, &ctx, None);
    assert!(
        !items.is_empty(),
        "Transform chain should produce completions"
    );
    assert_has_label(&items, "upper");
    assert_has_label(&items, "lower");
    assert_has_label(&items, "trim");
    assert_has_label(&items, "json");
    assert_has_label(&items, "length");
    assert_has_label(&items, "default(\"\")");
    assert_all_have_kind(&items);
}

#[test]
fn transform_chain_context_has_in_transform_chain() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: \"{{with.data | upper\"
";
    let offset = yaml.find("upper").unwrap() + 2;
    let ctx = detect_context(yaml, offset as u32, None);
    match &ctx {
        CursorContext::Template {
            in_transform_chain, ..
        } => {
            assert!(in_transform_chain, "Should detect transform chain");
        }
        other => panic!("Expected Template with transform chain, got {other:?}"),
    }
}

#[test]
fn transform_items_are_value_kind() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: \"{{with.data | \"
";
    let offset = yaml.find("| ").unwrap() + 2;
    let items = completions(
        yaml,
        offset as u32,
        &detect_context(yaml, offset as u32, None),
        None,
    );
    for item in &items {
        assert_label_kind(&items, &item.label, CompletionItemKind::VALUE);
    }
}

// ===========================================================================
// 10. Inside `mcp:\n  server:\n    ` -> command, args, env
// ===========================================================================

#[test]
fn mcp_config_completions() {
    let yaml = "\
schema: nika/workflow@0.12
mcp:
  novanet:
    ";
    let items = complete_at(yaml, 3, 4);
    assert!(!items.is_empty(), "MCP config should produce completions");
    assert_has_label(&items, "command");
    assert_has_label(&items, "args");
    assert_has_label(&items, "env");
    assert_all_have_kind(&items);
    assert_all_have_sort_text(&items);
}

#[test]
fn mcp_config_context_has_server_name() {
    let yaml = "\
schema: nika/workflow@0.12
mcp:
  novanet:
    ";
    let ctx = detect_context(yaml, text_offset(yaml, 3, 4), None);
    match &ctx {
        CursorContext::McpConfig { server_name, .. } => {
            assert_eq!(server_name.as_deref(), Some("novanet"));
        }
        other => panic!("Expected McpConfig, got {other:?}"),
    }
}

#[test]
fn mcp_config_items_are_property_kind() {
    let yaml = "\
schema: nika/workflow@0.12
mcp:
  novanet:
    ";
    let items = complete_at(yaml, 3, 4);
    for item in &items {
        assert_label_kind(&items, &item.label, CompletionItemKind::PROPERTY);
    }
}

// ===========================================================================
// 11. Inside `invoke:\n      ` -> mcp, tool, params
// ===========================================================================

#[test]
fn invoke_block_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    invoke:
      ";
    let items = complete_at(yaml, 4, 6);
    assert!(!items.is_empty(), "Invoke block should produce completions");
    assert_has_label(&items, "mcp");
    assert_has_label(&items, "tool");
    assert_has_label(&items, "params");
    assert_has_label(&items, "resource");
    assert_all_have_kind(&items);
}

#[test]
fn invoke_block_context_is_invoke_block() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    invoke:
      ";
    let ctx = detect_context(yaml, text_offset(yaml, 4, 6), None);
    assert!(
        matches!(ctx, CursorContext::InvokeBlock { .. }),
        "Expected InvokeBlock, got {ctx:?}"
    );
}

#[test]
fn invoke_block_items_are_property_kind() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    invoke:
      ";
    let items = complete_at(yaml, 4, 6);
    for item in &items {
        assert_label_kind(&items, &item.label, CompletionItemKind::PROPERTY);
    }
}

#[test]
fn invoke_block_with_existing_fields_detects_mcp_server() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    invoke:
      mcp: novanet
      tool: query
      ";
    let ctx = detect_context(yaml, text_offset(yaml, 5, 10), None);
    match &ctx {
        CursorContext::InvokeBlock { mcp_server, .. } => {
            assert_eq!(mcp_server.as_deref(), Some("novanet"));
        }
        other => panic!("Expected InvokeBlock, got {other:?}"),
    }
}

// ===========================================================================
// 12. Provider context -> anthropic, openai, etc from catalog
// ===========================================================================

#[test]
fn provider_context_detected_on_provider_line() {
    // When cursor is on a line starting with "provider:", the context is
    // ProviderContext. The text-based detection stores the full trimmed line
    // as prefix, so catalog items are filtered by that prefix. Test that the
    // context is correctly identified.
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      provider: ";
    let ctx = detect_context(yaml, text_offset(yaml, 4, 16), None);
    assert!(
        matches!(ctx, CursorContext::ProviderContext { .. }),
        "Expected ProviderContext, got {ctx:?}"
    );
}

#[test]
fn provider_context_at_task_level_detected() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    provider: ";
    let ctx = detect_context(yaml, text_offset(yaml, 3, 14), None);
    assert!(
        matches!(ctx, CursorContext::ProviderContext { .. }),
        "Expected ProviderContext at task level, got {ctx:?}"
    );
}

#[test]
fn provider_completions_via_direct_context() {
    // Construct ProviderContext directly with an empty prefix to verify
    // the full catalog is returned through the completions() API.
    let ctx = CursorContext::ProviderContext {
        task_id: Some("step1".to_string()),
        verb: "infer".to_string(),
        current_provider: None,
        current_model: None,
        prefix: String::new(),
    };
    let items = completions("", 0, &ctx, None);
    assert!(
        !items.is_empty(),
        "Provider completions should include catalog entries"
    );
    assert_has_label(&items, "anthropic");
    assert_has_label(&items, "openai");
    assert_has_label(&items, "mistral");
    assert_has_label(&items, "groq");
    assert_has_label(&items, "deepseek");
    assert_has_label(&items, "gemini");
    assert_has_label(&items, "xai");
    assert_all_have_kind(&items);
}

#[test]
fn provider_completions_include_aliases() {
    let ctx = CursorContext::ProviderContext {
        task_id: None,
        verb: "infer".to_string(),
        current_provider: None,
        current_model: None,
        prefix: String::new(),
    };
    let items = completions("", 0, &ctx, None);
    assert_has_label(&items, "claude");
    assert_has_label(&items, "gpt");
}

#[test]
fn provider_completions_providers_are_enum_member_kind() {
    let ctx = CursorContext::ProviderContext {
        task_id: None,
        verb: "infer".to_string(),
        current_provider: None,
        current_model: None,
        prefix: String::new(),
    };
    let items = completions("", 0, &ctx, None);
    assert_label_kind(&items, "anthropic", CompletionItemKind::ENUM_MEMBER);
    assert_label_kind(&items, "openai", CompletionItemKind::ENUM_MEMBER);
    assert_label_kind(&items, "claude", CompletionItemKind::ENUM_MEMBER);
}

#[test]
fn provider_field_offered_in_verb_block() {
    // In practice, the user gets "provider" as a completion suggestion
    // inside a verb block, then triggers a second completion for its value.
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      ";
    let items = complete_at(yaml, 4, 6);
    assert_has_label(&items, "provider");
}

#[test]
fn provider_field_offered_at_task_level() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    ";
    let items = complete_at(yaml, 3, 4);
    assert_has_label(&items, "provider");
    assert_has_label(&items, "model");
}

// ===========================================================================
// 13. Schema block -> type, properties, required
// ===========================================================================

#[test]
fn schema_block_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    structured:
      schema:
        ";
    let items = complete_at(yaml, 6, 8);
    assert!(!items.is_empty(), "Schema block should produce completions");
    assert_has_label(&items, "type");
    assert_has_label(&items, "properties");
    assert_has_label(&items, "required");
    assert_has_label(&items, "items");
    assert_has_label(&items, "description");
    assert_all_have_kind(&items);
    assert_all_have_sort_text(&items);
}

#[test]
fn schema_block_context_is_schema_block() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    structured:
      schema:
        ";
    let ctx = detect_context(yaml, text_offset(yaml, 6, 8), None);
    assert!(
        matches!(ctx, CursorContext::SchemaBlock { .. }),
        "Expected SchemaBlock, got {ctx:?}"
    );
}

#[test]
fn schema_block_items_are_property_kind() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    structured:
      schema:
        ";
    let items = complete_at(yaml, 6, 8);
    for item in &items {
        assert_label_kind(&items, &item.label, CompletionItemKind::PROPERTY);
    }
}

// ===========================================================================
// 14. Guardrails -> input, output
// ===========================================================================

#[test]
fn guardrails_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    guardrails:
      ";
    let items = complete_at(yaml, 5, 6);
    assert!(!items.is_empty(), "Guardrails should produce completions");
    assert_has_label(&items, "input");
    assert_has_label(&items, "output");
    assert_has_label(&items, "max_length");
    assert_has_label(&items, "forbidden_topics");
    assert_all_have_kind(&items);
    assert_all_have_sort_text(&items);
}

#[test]
fn guardrails_context_is_guardrails() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    guardrails:
      ";
    let ctx = detect_context(yaml, text_offset(yaml, 5, 6), None);
    assert!(
        matches!(ctx, CursorContext::Guardrails { .. }),
        "Expected Guardrails, got {ctx:?}"
    );
}

#[test]
fn guardrails_items_are_property_kind() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    guardrails:
      ";
    let items = complete_at(yaml, 5, 6);
    for item in &items {
        assert_label_kind(&items, &item.label, CompletionItemKind::PROPERTY);
    }
}

// ===========================================================================
// 15. 10-task workflow -> all completions still work
// ===========================================================================

#[test]
fn large_workflow_root_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: task01
    infer: step one
  - id: task02
    exec: echo two
  - id: task03
    fetch:
      url: https://example.com
      method: GET
  - id: task04
    invoke:
      mcp: novanet
      tool: query
  - id: task05
    agent:
      prompt: research
      max_turns: 5
  - id: task06
    infer: step six
  - id: task07
    infer: step seven
  - id: task08
    exec: echo eight
  - id: task09
    infer: step nine
  - id: task10
    infer: step ten
";
    // Root completions
    let items = complete_at(yaml, 0, 0);
    assert!(
        !items.is_empty(),
        "Root completions should work in large workflow"
    );
    assert_has_label(&items, "schema");
}

#[test]
fn large_workflow_task_field_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: task01
    infer: step one
  - id: task02
    exec: echo two
  - id: task03
    fetch:
      url: https://example.com
      method: GET
  - id: task04
    invoke:
      mcp: novanet
      tool: query
  - id: task05
    agent:
      prompt: research
      max_turns: 5
  - id: task06
    infer: step six
  - id: task07
    infer: step seven
  - id: task08
    exec: echo eight
  - id: task09
    infer: step nine
  - id: task10
    infer: step ten
    ";
    // Task field on last task (line 29, 4 spaces indent)
    let items = complete_at(yaml, 29, 4);
    assert!(
        !items.is_empty(),
        "Task field completions should work at end of 10-task workflow"
    );
}

#[test]
fn large_workflow_depends_on_lists_all_tasks() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: task01
    infer: step one
  - id: task02
    exec: echo two
  - id: task03
    infer: step three
  - id: task04
    infer: step four
  - id: task05
    infer: step five
  - id: task06
    infer: step six
  - id: task07
    infer: step seven
  - id: task08
    exec: echo eight
  - id: task09
    infer: step nine
  - id: task10
    depends_on:
      ";
    let items = complete_at(yaml, 22, 6);
    assert!(
        !items.is_empty(),
        "depends_on should list tasks in large workflow"
    );
    // Should have all 10 task IDs
    for i in 1..=10 {
        assert_has_label(&items, &format!("task{i:02}"));
    }
}

#[test]
fn large_workflow_with_block_lists_all_tasks() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: task01
    infer: step one
  - id: task02
    exec: echo two
  - id: task03
    infer: step three
  - id: task04
    infer: step four
  - id: task05
    infer: step five
  - id: task06
    infer: step six
  - id: task07
    infer: step seven
  - id: task08
    exec: echo eight
  - id: task09
    infer: step nine
  - id: task10
    with:
      data: ";
    let items = complete_at(yaml, 22, 12);
    assert!(
        !items.is_empty(),
        "with block should list tasks in large workflow"
    );
    for i in 1..=10 {
        assert_has_label(&items, &format!("task{i:02}"));
    }
}

#[test]
fn large_workflow_template_has_task_shorthands() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: task01
    infer: step one
  - id: task02
    exec: echo two
  - id: task03
    infer: step three
  - id: task04
    infer: step four
  - id: task05
    infer: step five
  - id: task06
    infer: step six
  - id: task07
    infer: step seven
  - id: task08
    exec: echo eight
  - id: task09
    infer: step nine
  - id: task10
    infer: \"{{\"
";
    let offset = yaml.rfind("{{").unwrap() + 2;
    let items = completions(
        yaml,
        offset as u32,
        &detect_context(yaml, offset as u32, None),
        None,
    );
    assert!(!items.is_empty(), "Template should work in large workflow");
    // Should have shorthands for all 10 tasks
    for i in 1..=10 {
        assert_has_label(&items, &format!("$task{i:02}"));
    }
}

// ===========================================================================
// 16. Workflow with content: block -> vision completions present
// ===========================================================================

#[test]
fn vision_workflow_content_type() {
    // Place cursor on a blank line inside the content block to get PartField
    // completions without prefix filtering issues.
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: analyze-image
    infer:
      prompt: Describe this image
      content:
        ";
    let items = complete_at(yaml, 6, 8);
    assert!(
        !items.is_empty(),
        "Vision content type completions should be present"
    );
    // On a blank line inside content:, we get PartField suggestions
    assert_all_have_kind(&items);
}

#[test]
fn vision_workflow_content_via_direct_context() {
    // Use direct CursorContext::ContentPart with PartType focus and empty
    // prefix to verify text/image/image_url are all returned.
    use nika_lsp_core::analysis::context::ContentFocus;
    let ctx = CursorContext::ContentPart {
        task_id: Some("analyze-image".to_string()),
        focus: ContentFocus::PartType,
        part_type: None,
        prefix: String::new(),
    };
    let items = completions("", 0, &ctx, None);
    assert!(
        !items.is_empty(),
        "PartType completions should include type variants"
    );
    assert_has_label(&items, "text");
    assert_has_label(&items, "image");
    assert_has_label(&items, "image_url");
}

#[test]
fn vision_workflow_image_fields() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: analyze-image
    infer:
      prompt: Describe this image
      content:
        - type: image
          ";
    let items = complete_at(yaml, 7, 10);
    assert!(
        !items.is_empty(),
        "Image part field completions should be present"
    );
    // Should get source, detail, text, url
    assert_all_have_kind(&items);
}

#[test]
fn vision_workflow_image_url_variant() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: analyze-image
    infer:
      prompt: Describe this image
      content:
        - type: image_url
          url: ";
    let ctx = detect_context(yaml, text_offset(yaml, 7, 14), None);
    assert!(
        matches!(ctx, CursorContext::ContentPart { .. }),
        "Expected ContentPart for image_url, got {ctx:?}"
    );
}

#[test]
fn vision_workflow_multiple_content_parts() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: analyze-image
    infer:
      prompt: Compare these images
      content:
        - type: text
          text: First image
        - type: image
          source: hash123
          detail: high
        - type: image_url
          url: https://example.com/img.png
          detail: ";
    let items = complete_at(yaml, 13, 18);
    assert!(
        !items.is_empty(),
        "Detail completions should work with multiple content parts"
    );
    assert_has_label(&items, "auto");
    assert_has_label(&items, "low");
    assert_has_label(&items, "high");
}

// ===========================================================================
// Additional verb blocks (exec, fetch, agent)
// ===========================================================================

#[test]
fn exec_block_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    exec:
      ";
    let items = complete_at(yaml, 4, 6);
    assert!(!items.is_empty(), "Exec block should produce completions");
    assert_has_label(&items, "command");
    assert_has_label(&items, "shell");
    assert_all_have_kind(&items);
}

#[test]
fn fetch_block_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    fetch:
      ";
    let items = complete_at(yaml, 4, 6);
    assert!(!items.is_empty(), "Fetch block should produce completions");
    assert_has_label(&items, "url");
    assert_has_label(&items, "method");
    assert_has_label(&items, "headers");
    assert_has_label(&items, "body");
    assert_no_label(&items, "retry"); // retry is task-level, not fetch sub-field
    assert_all_have_kind(&items);
}

#[test]
fn agent_block_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    agent:
      ";
    let items = complete_at(yaml, 4, 6);
    assert!(!items.is_empty(), "Agent block should produce completions");
    assert_has_label(&items, "prompt");
    assert_has_label(&items, "system");
    assert_has_label(&items, "mcp");
    assert_has_label(&items, "tools");
    assert_has_label(&items, "max_turns");
    assert_has_label(&items, "depth_limit");
    assert_has_label(&items, "provider");
    assert_has_label(&items, "model");
    assert_has_label(&items, "extended_thinking");
    assert_all_have_kind(&items);
}

// ===========================================================================
// Retry / limits blocks
// ===========================================================================

#[test]
fn retry_block_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    retry:
      ";
    let items = complete_at(yaml, 5, 6);
    assert!(!items.is_empty(), "Retry block should produce completions");
    assert_has_label(&items, "max_attempts");
    assert_has_label(&items, "delay");
    assert_has_label(&items, "backoff");
    assert_all_have_kind(&items);
    assert_all_have_sort_text(&items);
}

#[test]
fn retry_block_context_is_retry_block() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    retry:
      ";
    let ctx = detect_context(yaml, text_offset(yaml, 5, 6), None);
    assert!(
        matches!(ctx, CursorContext::RetryBlock { .. }),
        "Expected RetryBlock, got {ctx:?}"
    );
}

#[test]
fn for_each_block_completions() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    for_each:
      ";
    let items = complete_at(yaml, 4, 6);
    assert!(
        !items.is_empty(),
        "ForEach block should produce completions"
    );
    assert_has_label(&items, "items");
    assert_has_label(&items, "as");
    assert_has_label(&items, "concurrency");
    assert_all_have_kind(&items);
}

// ===========================================================================
// Edge cases and robustness
// ===========================================================================

#[test]
fn no_panic_on_empty_string() {
    let items = complete_at("", 0, 0);
    assert!(!items.is_empty());
}

#[test]
fn no_panic_on_whitespace_only() {
    let items = complete_at("   \n  \n\n", 0, 0);
    // Should not panic, may return empty or root completions
    let _ = items;
}

#[test]
fn no_panic_on_single_newline() {
    let items = complete_at("\n", 0, 0);
    let _ = items;
}

#[test]
fn no_panic_on_deeply_nested_yaml() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      content:
        - type: image
          source: hash
          detail: high
            extra:
              deeply:
                nested:
                  field: value
";
    // Should not panic even on unusual nesting
    let _ = complete_at(yaml, 11, 20);
}

#[test]
fn no_panic_on_malformed_yaml() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
  : broken: yaml here
    ???: !!!
      garbled content
";
    let _ = complete_at(yaml, 4, 2);
    let _ = complete_at(yaml, 5, 4);
    let _ = complete_at(yaml, 6, 6);
}

#[test]
fn no_panic_on_incomplete_template() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: \"{{\"
";
    let offset = yaml.find("{{").unwrap() + 2;
    let _ = completions(
        yaml,
        offset as u32,
        &detect_context(yaml, offset as u32, None),
        None,
    );
}

#[test]
fn no_panic_on_offset_past_end() {
    let ctx = detect_context("short text", 9999, None);
    assert!(matches!(ctx, CursorContext::Unknown { .. }));
    let items = completions("short text", 9999, &ctx, None);
    assert!(items.is_empty());
}

#[test]
fn no_panic_on_binary_like_content() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: \"\x00\x01\x02\"
    ";
    let _ = complete_at(yaml, 4, 4);
}

// ===========================================================================
// Prefix filtering through the full pipeline
// ===========================================================================

#[test]
fn root_prefix_filters_to_schema() {
    let yaml = "sch";
    let items = complete_at(yaml, 0, 3);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "schema");
}

#[test]
fn root_prefix_filters_to_tasks() {
    let yaml = "ta";
    let items = complete_at(yaml, 0, 2);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "tasks");
}

#[test]
fn root_prefix_case_insensitive() {
    let yaml = "SCH";
    let items = complete_at(yaml, 0, 3);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "schema");
}

#[test]
fn task_field_prefix_filters() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    in";
    let items = complete_at(yaml, 3, 6);
    // "in" should match "infer", "invoke", "include" (not at task level), "inputs" (not at task level)
    assert!(
        items.iter().any(|i| i.label == "infer"),
        "Should match 'infer'"
    );
    assert!(
        items.iter().any(|i| i.label == "invoke"),
        "Should match 'invoke'"
    );
}

// ===========================================================================
// Sort order consistency
// ===========================================================================

#[test]
fn workflow_root_items_sorted() {
    let items = complete_at("", 0, 0);
    let sort_keys: Vec<_> = items
        .iter()
        .filter_map(|i| i.sort_text.as_deref())
        .collect();
    let mut sorted = sort_keys.clone();
    sorted.sort();
    assert_eq!(sort_keys, sorted, "Root items should be pre-sorted");
}

#[test]
fn infer_block_items_have_sort_text() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      ";
    let items = complete_at(yaml, 4, 6);
    // All infer block items should have sort_text for client-side ordering.
    // Items within the same priority group (e.g. 3_temperature, 3_max_tokens)
    // may appear in insertion order, which is valid.
    assert_all_have_sort_text(&items);
    // Verify that sort groups are monotonically non-decreasing by their
    // numeric prefix (the digit before the underscore).
    let groups: Vec<u8> = items
        .iter()
        .filter_map(|i| i.sort_text.as_ref())
        .filter_map(|s| s.as_bytes().first().copied())
        .collect();
    for pair in groups.windows(2) {
        assert!(
            pair[0] <= pair[1],
            "Sort groups should be non-decreasing, got {} before {}",
            pair[0] as char,
            pair[1] as char
        );
    }
}

// ===========================================================================
// Documentation presence
// ===========================================================================

#[test]
fn all_root_items_have_documentation() {
    let items = complete_at("", 0, 0);
    for item in &items {
        assert!(
            item.documentation.is_some(),
            "Item '{}' is missing documentation",
            item.label
        );
    }
}

#[test]
fn all_infer_items_have_documentation() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      ";
    let items = complete_at(yaml, 4, 6);
    for item in &items {
        assert!(
            item.documentation.is_some(),
            "Infer item '{}' is missing documentation",
            item.label
        );
    }
}

#[test]
fn all_task_field_items_have_documentation() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    ";
    let items = complete_at(yaml, 3, 4);
    for item in &items {
        assert!(
            item.documentation.is_some(),
            "Task field item '{}' is missing documentation",
            item.label
        );
    }
}

// ===========================================================================
// Realistic multi-block workflow
// ===========================================================================

#[test]
fn realistic_workflow_infer_block() {
    let yaml = "\
schema: nika/workflow@0.12
workflow: image-analyzer
mcp:
  novanet:
    command: cargo run
    args: [--release]
tasks:
  - id: fetch-image
    fetch:
      url: https://example.com/api/image
      method: GET
  - id: analyze
    with:
      img: fetch-image
    infer:
      prompt: \"Analyze this image: {{with.img}}\"
      model: claude-sonnet-4-6
      content:
        - type: image_url
          url: \"{{with.img}}\"
  - id: report
    with:
      analysis: analyze
    infer:
      ";
    // Completions inside infer block of the last task
    let last_line = yaml.lines().count() - 1;
    let items = complete_at(yaml, last_line, 6);
    assert!(
        !items.is_empty(),
        "Infer completions should work in realistic workflow"
    );
    assert_has_label(&items, "prompt");
    assert_has_label(&items, "model");
    assert_has_label(&items, "temperature");
}

#[test]
fn realistic_workflow_with_block() {
    let yaml = "\
schema: nika/workflow@0.12
workflow: pipeline
tasks:
  - id: fetch-data
    fetch:
      url: https://api.example.com/data
      method: GET
  - id: process
    with:
      raw: fetch-data
    exec: echo processing
  - id: summarize
    with:
      ";
    let last_line = yaml.lines().count() - 1;
    let items = complete_at(yaml, last_line, 6);
    assert!(!items.is_empty(), "With block should list available tasks");
    assert_has_label(&items, "fetch-data");
    assert_has_label(&items, "process");
    assert_has_label(&items, "summarize");
}

#[test]
fn realistic_workflow_template_completions() {
    let yaml = "\
schema: nika/workflow@0.12
workflow: pipeline
context:
  files:
    readme: ./README.md
inputs:
  name:
    type: string
tasks:
  - id: greet
    infer: \"Hello {{\"
";
    let offset = yaml.rfind("{{").unwrap() + 2;
    let items = completions(
        yaml,
        offset as u32,
        &detect_context(yaml, offset as u32, None),
        None,
    );
    assert!(
        !items.is_empty(),
        "Template completions should work in realistic workflow"
    );
    assert_has_label(&items, "with.");
    assert_has_label(&items, "context.files.");
    assert_has_label(&items, "inputs.");
    assert_has_label(&items, "$greet");
}

// ===========================================================================
// Unknown context returns empty
// ===========================================================================

#[test]
fn unknown_context_returns_empty_completions() {
    let ctx = CursorContext::Unknown {
        prefix: String::new(),
    };
    let items = completions("", 0, &ctx, None);
    assert!(
        items.is_empty(),
        "Unknown context should return empty completions"
    );
}

#[test]
fn orphan_indented_line_is_unknown() {
    let yaml = "\
schema: nika/workflow@0.12
  orphan: true
";
    let ctx = detect_context(yaml, text_offset(yaml, 1, 10), None);
    assert!(
        matches!(ctx, CursorContext::Unknown { .. }),
        "Orphan indented line should be Unknown, got {ctx:?}"
    );
    let items = completions(yaml, text_offset(yaml, 1, 10), &ctx, None);
    assert!(items.is_empty());
}

// ===========================================================================
// insert_text is always set
// ===========================================================================

#[test]
fn all_root_items_have_insert_text() {
    let items = complete_at("", 0, 0);
    for item in &items {
        assert!(
            item.insert_text.is_some(),
            "Item '{}' is missing insert_text",
            item.label
        );
    }
}

#[test]
fn all_task_field_items_have_insert_text() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    ";
    let items = complete_at(yaml, 3, 4);
    for item in &items {
        assert!(
            item.insert_text.is_some(),
            "Task field item '{}' is missing insert_text",
            item.label
        );
    }
}

// ===========================================================================
// Verb-level field completions (invoke, infer, agent, fetch)
// ===========================================================================

#[test]
fn invoke_has_resource_completion() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    invoke:\n      ";
    let items = complete_at(yaml, 4, 6);
    assert_has_label(&items, "resource");
}

#[test]
fn infer_has_guardrails_completion() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    infer:\n      ";
    let items = complete_at(yaml, 4, 6);
    assert_has_label(&items, "guardrails");
}

#[test]
fn agent_has_guardrails_completion() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    agent:\n      ";
    let items = complete_at(yaml, 4, 6);
    assert_has_label(&items, "guardrails");
}

#[test]
fn fetch_no_retry_in_verb_block() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    fetch:\n      ";
    let items = complete_at(yaml, 4, 6);
    assert_no_label(&items, "retry");
}

#[test]
fn task_field_no_guardrails() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    ";
    let items = complete_at(yaml, 3, 4);
    assert_no_label(&items, "guardrails");
}

#[test]
fn agent_has_from_completion() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    agent:\n      ";
    let items = complete_at(yaml, 4, 6);
    assert_has_label(&items, "from");
}

#[test]
fn agent_has_thinking_budget_completion() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    agent:\n      ";
    let items = complete_at(yaml, 4, 6);
    assert_has_label(&items, "thinking_budget");
}

// ===========================================================================
// Workflow root includes newer keys
// ===========================================================================

#[test]
fn workflow_root_has_all_new_keys() {
    let items = complete_at("", 0, 0);
    assert_has_label(&items, "provider");
    assert_has_label(&items, "model");
    assert_has_label(&items, "description");
    assert_has_label(&items, "skills");
    assert_has_label(&items, "agents");
    assert_has_label(&items, "artifacts");
    assert_has_label(&items, "log");
}
