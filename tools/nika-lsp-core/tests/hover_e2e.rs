// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! End-to-end integration tests for the hover handler.
//!
//! Each test constructs a realistic `.nika.yaml` snippet, computes the byte
//! offset for a cursor position, runs context detection followed by
//! `handlers::hover::hover`, and asserts the result contains non-empty
//! markdown documentation (or `None` for positions that should yield nothing).

use nika_lsp_core::analysis::context::{detect_context, ContentFocus, CursorContext};
use nika_lsp_core::handlers::hover::{hover, HoverResult};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert 0-based (line, character) to byte offset within `text`.
fn byte_offset(text: &str, line: usize, character: usize) -> u32 {
    let mut offset = 0usize;
    for (i, l) in text.lines().enumerate() {
        if i == line {
            return (offset + character.min(l.len())) as u32;
        }
        offset += l.len() + 1; // +1 for '\n'
    }
    text.len() as u32
}

/// Run the full pipeline: detect_context -> hover.
fn hover_at(text: &str, line: usize, character: usize) -> Option<HoverResult> {
    let off = byte_offset(text, line, character);
    let ctx = detect_context(text, off, None);
    hover(text, off, &ctx, None)
}

/// Assert `hover_at` returns `Some` with non-empty markdown content.
fn assert_hover(text: &str, line: usize, character: usize, description: &str) {
    let result = hover_at(text, line, character);
    assert!(
        result.is_some(),
        "Expected hover content for {description}, got None"
    );
    let contents = &result.unwrap().contents;
    assert!(
        !contents.is_empty(),
        "Hover contents must be non-empty for {description}"
    );
}

/// Assert `hover_at` returns `None`.
fn assert_no_hover(text: &str, line: usize, character: usize, description: &str) {
    let result = hover_at(text, line, character);
    assert!(
        result.is_none(),
        "Expected no hover for {description}, got: {:?}",
        result.map(|r| r.contents)
    );
}

// ===========================================================================
// 1. Verb hover -- all 5 verbs
// ===========================================================================

#[test]
fn hover_verb_infer() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: gen
    infer:
      prompt: hello
      ";
    // Cursor inside the infer: sub-block (indent 6).
    assert_hover(yaml, 4, 6, "infer verb block");
}

#[test]
fn hover_verb_exec() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: run
    exec:
      command: echo ok
      ";
    assert_hover(yaml, 4, 6, "exec verb block");
}

#[test]
fn hover_verb_fetch() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: req
    fetch:
      url: https://example.com
      ";
    assert_hover(yaml, 4, 6, "fetch verb block");
}

#[test]
fn hover_verb_invoke() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: call
    invoke:
      mcp: novanet
      tool: search
      ";
    // invoke: context resolves to InvokeBlock, not VerbBlock, so hover
    // dispatches through the catch-all. We test by constructing a VerbBlock
    // directly to verify verb_hover covers "invoke".
    let off = byte_offset(yaml, 4, 10);
    let ctx = detect_context(yaml, off, None);
    // InvokeBlock is not handled by hover -> returns None.
    // Test verb_hover via a synthetic VerbBlock.
    let verb_ctx = CursorContext::VerbBlock {
        task_id: Some("call".into()),
        verb: "invoke".into(),
        existing_subfields: vec![],
        prefix: String::new(),
    };
    let result = hover(yaml, 0, &verb_ctx, None);
    assert!(result.is_some(), "invoke verb hover must return Some");
    assert!(
        result.unwrap().contents.contains("MCP"),
        "invoke hover doc should mention MCP"
    );
    // Also assert the auto-detected context still exists.
    assert!(
        matches!(ctx, CursorContext::InvokeBlock { .. }),
        "Context should be InvokeBlock, got {ctx:?}"
    );
}

#[test]
fn hover_verb_agent() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: bot
    agent:
      prompt: research
      ";
    assert_hover(yaml, 4, 6, "agent verb block");
}

// ===========================================================================
// 2. Task fields
// ===========================================================================

#[test]
fn hover_task_field_id() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
";
    // Construct TaskField directly to test field_hover for "id".
    let field_ctx = CursorContext::TaskField {
        task_id: Some("step1".into()),
        existing_fields: vec![],
        prefix: "id:".into(),
    };
    let r = hover(yaml, 0, &field_ctx, None);
    assert!(r.is_some(), "hover on 'id' field should return doc");
    assert!(r.unwrap().contents.contains("Task Identifier"));
}

#[test]
fn hover_task_field_with() {
    let ctx = CursorContext::TaskField {
        task_id: None,
        existing_fields: vec![],
        prefix: "with:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "hover on 'with' task field");
    assert!(r.unwrap().contents.contains("Bindings"));
}

#[test]
fn hover_task_field_depends_on() {
    let ctx = CursorContext::TaskField {
        task_id: None,
        existing_fields: vec![],
        prefix: "depends_on:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "hover on 'depends_on' task field");
    assert!(r.unwrap().contents.contains("Dependencies"));
}

#[test]
fn hover_task_field_content() {
    let ctx = CursorContext::TaskField {
        task_id: None,
        existing_fields: vec![],
        prefix: "content:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "hover on 'content' task field");
    assert!(r.unwrap().contents.contains("Vision"));
}

#[test]
fn hover_task_field_for_each() {
    let ctx = CursorContext::TaskField {
        task_id: None,
        existing_fields: vec![],
        prefix: "for_each:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "hover on 'for_each' task field");
    assert!(r.unwrap().contents.contains("Parallel Iteration"));
}

#[test]
fn hover_task_field_retry() {
    let ctx = CursorContext::TaskField {
        task_id: None,
        existing_fields: vec![],
        prefix: "retry:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "hover on 'retry' task field");
    assert!(r.unwrap().contents.contains("Retry"));
}

#[test]
fn hover_task_field_timeout() {
    let ctx = CursorContext::TaskField {
        task_id: None,
        existing_fields: vec![],
        prefix: "timeout:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "hover on 'timeout' task field");
    assert!(r.unwrap().contents.contains("Timeout"));
}

// ===========================================================================
// 3. Root keys
// ===========================================================================

#[test]
fn hover_root_schema() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n";
    // Cursor at column 7 captures prefix "schema:" which strips to "schema".
    assert_hover(yaml, 0, 7, "root 'schema' key");
}

#[test]
fn hover_root_tasks() {
    let yaml = "schema: nika/workflow@0.12\ntasks:\n";
    // Cursor at column 6 captures prefix "tasks:" which strips to "tasks".
    assert_hover(yaml, 1, 6, "root 'tasks' key");
}

#[test]
fn hover_root_mcp() {
    let yaml = "schema: nika/workflow@0.12\nmcp:\n  novanet:\n    command: cargo\n";
    // Cursor at column 4 captures prefix "mcp:" which strips to "mcp".
    assert_hover(yaml, 1, 4, "root 'mcp' key");
}

// ===========================================================================
// 4. Content parts
// ===========================================================================

#[test]
fn hover_content_part_type() {
    let ctx = CursorContext::ContentPart {
        task_id: Some("vision".into()),
        focus: ContentFocus::PartType,
        part_type: None,
        prefix: "- type:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "hover on content PartType");
    assert!(r.unwrap().contents.contains("Content Type"));
}

#[test]
fn hover_content_image_detail() {
    let ctx = CursorContext::ContentPart {
        task_id: Some("vision".into()),
        focus: ContentFocus::ImageDetail,
        part_type: Some("image".into()),
        prefix: "detail:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "hover on content ImageDetail");
    assert!(r.unwrap().contents.contains("Detail"));
}

#[test]
fn hover_content_image_url() {
    let ctx = CursorContext::ContentPart {
        task_id: None,
        focus: ContentFocus::ImageUrl,
        part_type: Some("image_url".into()),
        prefix: "url:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "ImageUrl focus should return hover doc");
    assert!(r.unwrap().contents.contains("Image URL"));
}

#[test]
fn hover_content_part_field() {
    let ctx = CursorContext::ContentPart {
        task_id: None,
        focus: ContentFocus::PartField,
        part_type: None,
        prefix: "text:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "PartField focus should return hover doc");
    assert!(r.unwrap().contents.contains("Content Part"));
}

#[test]
fn hover_content_part_type_e2e() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: vision
    infer:
      content:
        - type: text
          text: describe this
";
    // Line 5: `        - type: text` -- cursor at column 16 captures
    // prefix "        - type: " which trimmed gives "- type:" matching PartType.
    assert_hover(yaml, 5, 16, "content part type e2e");
}

// ===========================================================================
// 5. Transforms
// ===========================================================================

#[test]
fn hover_transform_upper() {
    let ctx = CursorContext::Template {
        task_id: None,
        available_bindings: vec![],
        partial_expr: "with.data | upper".into(),
        in_transform_chain: true,
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "hover on 'upper' transform");
    let contents = r.unwrap().contents;
    assert!(contents.contains("upper"), "should mention 'upper'");
    assert!(contents.contains("UPPERCASE"), "should describe UPPERCASE");
}

#[test]
fn hover_transform_lower() {
    let ctx = CursorContext::Template {
        task_id: None,
        available_bindings: vec![],
        partial_expr: "with.x | lower".into(),
        in_transform_chain: true,
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "hover on 'lower' transform");
    assert!(r.unwrap().contents.contains("lowercase"));
}

#[test]
fn hover_transform_trim() {
    let ctx = CursorContext::Template {
        task_id: None,
        available_bindings: vec![],
        partial_expr: "with.x | trim".into(),
        in_transform_chain: true,
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "hover on 'trim' transform");
    assert!(r.unwrap().contents.contains("whitespace"));
}

#[test]
fn hover_transform_unknown_returns_none() {
    let ctx = CursorContext::Template {
        task_id: None,
        available_bindings: vec![],
        partial_expr: "with.x | nonexistent".into(),
        in_transform_chain: true,
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_none(), "unknown transform should return None");
}

#[test]
fn hover_transform_chained() {
    // Chained transforms: `upper | trim` -- hover shows the LAST one.
    let ctx = CursorContext::Template {
        task_id: None,
        available_bindings: vec![],
        partial_expr: "with.x | upper | trim".into(),
        in_transform_chain: true,
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "chained transform hover");
    assert!(
        r.unwrap().contents.contains("trim"),
        "should show last transform in chain"
    );
}

#[test]
fn hover_transform_upper_e2e() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: \"{{with.data | upper}}\"
";
    // Place cursor right after the full word "upper" so the partial_expr
    // contains "with.data | upper" and transform_hover matches "upper".
    let upper_offset = yaml.find("upper").unwrap();
    let off = (upper_offset + 5) as u32; // end of "upper"
    let ctx = detect_context(yaml, off, None);
    assert!(
        matches!(
            ctx,
            CursorContext::Template {
                in_transform_chain: true,
                ..
            }
        ),
        "Should detect transform chain, got {ctx:?}"
    );
    let r = hover(yaml, off, &ctx, None);
    assert!(r.is_some(), "upper transform e2e hover");
    assert!(r.unwrap().contents.contains("UPPERCASE"));
}

// ===========================================================================
// 6. With block
// ===========================================================================

#[test]
fn hover_with_block() {
    let ctx = CursorContext::WithBlock {
        task_id: Some("step2".into()),
        alias: Some("data".into()),
        partial_ref: "$step1".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "with block hover");
    assert!(r.unwrap().contents.contains("Binding"));
}

#[test]
fn hover_with_block_e2e() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    with:
      data: $step1
    infer: \"{{with.data}}\"
";
    // Line 6: `      data: $step1` -- inside the with: block.
    assert_hover(yaml, 6, 8, "with block e2e");
}

// ===========================================================================
// 7. Template (non-transform)
// ===========================================================================

#[test]
fn hover_template_expression() {
    let ctx = CursorContext::Template {
        task_id: None,
        available_bindings: vec!["data".into()],
        partial_expr: "with.da".into(),
        in_transform_chain: false,
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "template expression hover");
    assert!(r.unwrap().contents.contains("Binding Reference"));
}

#[test]
fn hover_template_expression_e2e() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    with:
      prev: $step1
    infer: \"{{with.prev}}\"
";
    // Position cursor inside {{with.prev}} -- on "with".
    let tmpl_start = yaml.find("{{with.prev").unwrap() + 2; // after "{{"
    let off = (tmpl_start + 2) as u32; // "wi|th"
    let ctx = detect_context(yaml, off, None);
    assert!(
        matches!(
            ctx,
            CursorContext::Template {
                in_transform_chain: false,
                ..
            }
        ),
        "Should be Template without transform chain, got {ctx:?}"
    );
    let r = hover(yaml, off, &ctx, None);
    assert!(r.is_some(), "template expression e2e hover");
}

// ===========================================================================
// 8. Unknown / None positions
// ===========================================================================

#[test]
fn hover_unknown_context_returns_none() {
    let ctx = CursorContext::Unknown {
        prefix: "random".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_none(), "Unknown context should produce no hover");
}

#[test]
fn hover_empty_document_returns_none() {
    // Empty doc => WorkflowRoot with empty prefix => root_key_hover("") => None.
    let r = hover_at("", 0, 0);
    assert!(r.is_none(), "empty document should produce no hover");
}

#[test]
fn hover_unknown_root_key_returns_none() {
    let yaml = "custom_key: value\n";
    let r = hover_at(yaml, 0, 5);
    // WorkflowRoot with prefix "custo" -- root_key_hover("custo") => None.
    assert!(r.is_none(), "unknown root key should return None");
}

#[test]
fn hover_unknown_task_field_returns_none() {
    let ctx = CursorContext::TaskField {
        task_id: None,
        existing_fields: vec![],
        prefix: "unknown_field:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_none(), "unknown task field should return None");
}

#[test]
fn hover_offset_past_end_returns_none() {
    let r = hover_at("short", 99, 99);
    assert!(r.is_none(), "offset past document end should return None");
}

// ===========================================================================
// 9. Unhandled context variants return None
// ===========================================================================

#[test]
fn hover_depends_on_context() {
    let ctx = CursorContext::DependsOn {
        task_id: None,
        existing_deps: vec!["step1".into()],
        prefix: "- ste".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "DependsOn context should return hover doc");
    assert!(r.unwrap().contents.contains("Execution Dependencies"));
}

#[test]
fn hover_for_each_context() {
    let ctx = CursorContext::ForEach {
        task_id: None,
        loop_var: Some("item".into()),
        prefix: "items:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "ForEach context should return hover doc");
    assert!(r.unwrap().contents.contains("Parallel Iteration"));
}

#[test]
fn hover_mcp_config() {
    let ctx = CursorContext::McpConfig {
        server_name: Some("novanet".into()),
        prefix: "command:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "McpConfig should return MCP hover doc");
    assert!(r.unwrap().contents.contains("MCP"));
}

#[test]
fn hover_invoke_block() {
    use nika_lsp_core::analysis::context::InvokeFocus;
    let ctx = CursorContext::InvokeBlock {
        task_id: None,
        mcp_server: Some("novanet".into()),
        tool_name: Some("search".into()),
        focus: InvokeFocus::General,
        prefix: String::new(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "InvokeBlock should return invoke verb hover");
    assert!(r.unwrap().contents.contains("MCP"));
}

// ===========================================================================
// 10. Full workflow -- realistic multi-task document
// ===========================================================================

#[test]
fn hover_full_workflow_smoke() {
    let yaml = "\
schema: nika/workflow@0.12
mcp:
  novanet:
    command: cargo run
tasks:
  - id: gather
    fetch:
      url: https://api.example.com/data
      method: GET
  - id: process
    with:
      raw: $gather
    exec:
      command: jq '.items[]'
  - id: summarize
    with:
      processed: $process
    infer:
      prompt: \"Summarize: {{with.processed | trim}}\"
      content:
        - type: text
          text: \"{{with.processed}}\"
  - id: report
    depends_on: [summarize]
    agent:
      prompt: Write a report
      tools:
        - mcp: novanet
          tool: write
";

    // Root key: schema (line 0) -- cursor at col 7 to capture "schema:"
    assert_hover(yaml, 0, 7, "full workflow: schema root key");

    // Root key: tasks (line 4) -- cursor at col 6 to capture "tasks:"
    assert_hover(yaml, 4, 6, "full workflow: tasks root key");

    // fetch verb (line 7, inside fetch: block)
    assert_hover(yaml, 7, 6, "full workflow: fetch verb");

    // with: block (line 11, `raw: $gather`)
    assert_hover(yaml, 11, 8, "full workflow: with block");

    // infer verb (line 18, inside infer: block -- `prompt:` sub-field)
    assert_hover(yaml, 18, 6, "full workflow: infer verb");

    // content part type (line 20, `- type: text` -- cursor at col 16)
    assert_hover(yaml, 20, 16, "full workflow: content part type");

    // agent verb (line 25, inside agent: block -- `prompt:` sub-field)
    assert_hover(yaml, 25, 6, "full workflow: agent verb");

    // Unknown position past the schema value
    assert_no_hover(yaml, 0, 20, "full workflow: past schema value");
}

// ===========================================================================
// 11. Verb sub-field hover
// ===========================================================================

#[test]
fn hover_verb_subfield_prompt_in_infer() {
    let ctx = CursorContext::VerbBlock {
        task_id: Some("step1".into()),
        verb: "infer".into(),
        existing_subfields: vec![],
        prefix: "prompt:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "prompt sub-field should have hover");
    let contents = r.unwrap().contents;
    assert!(
        contents.contains("Text Prompt") || contents.contains("prompt"),
        "prompt hover should mention 'Text Prompt' or 'prompt', got: {contents}"
    );
}

#[test]
fn hover_verb_subfield_extract_in_fetch() {
    let ctx = CursorContext::VerbBlock {
        task_id: Some("req".into()),
        verb: "fetch".into(),
        existing_subfields: vec![],
        prefix: "extract:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "extract sub-field should have hover");
    let contents = r.unwrap().contents;
    assert!(
        contents.contains("Post-Processing") || contents.contains("extraction"),
        "extract hover should mention 'Post-Processing' or 'extraction', got: {contents}"
    );
}

#[test]
fn hover_verb_subfield_model_in_agent() {
    let ctx = CursorContext::VerbBlock {
        task_id: Some("bot".into()),
        verb: "agent".into(),
        existing_subfields: vec![],
        prefix: "model:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "model sub-field should have hover");
    let contents = r.unwrap().contents;
    assert!(
        contents.contains("Model Override") || contents.contains("claude"),
        "model hover should mention 'Model Override' or 'claude', got: {contents}"
    );
}

#[test]
fn hover_verb_subfield_tool_in_invoke() {
    let ctx = CursorContext::VerbBlock {
        task_id: Some("call".into()),
        verb: "invoke".into(),
        existing_subfields: vec![],
        prefix: "tool:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "tool sub-field should have hover");
    let contents = r.unwrap().contents;
    assert!(
        contents.contains("MCP Tool"),
        "tool hover should mention 'MCP Tool', got: {contents}"
    );
}

#[test]
fn hover_verb_subfield_max_turns_in_agent() {
    let ctx = CursorContext::VerbBlock {
        task_id: Some("bot".into()),
        verb: "agent".into(),
        existing_subfields: vec![],
        prefix: "max_turns:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "max_turns sub-field should have hover");
    let contents = r.unwrap().contents;
    assert!(
        contents.contains("Maximum") || contents.contains("iterations"),
        "max_turns hover should mention 'Maximum' or 'iterations', got: {contents}"
    );
}

#[test]
fn hover_verb_subfield_extended_thinking_in_infer() {
    let ctx = CursorContext::VerbBlock {
        task_id: Some("step1".into()),
        verb: "infer".into(),
        existing_subfields: vec![],
        prefix: "extended_thinking:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "extended_thinking sub-field should have hover");
    let contents = r.unwrap().contents;
    assert!(
        contents.contains("Chain-of-Thought") || contents.contains("Claude"),
        "extended_thinking hover should mention 'Chain-of-Thought' or 'Claude', got: {contents}"
    );
}

#[test]
fn hover_verb_subfield_command_in_exec() {
    let ctx = CursorContext::VerbBlock {
        task_id: Some("run".into()),
        verb: "exec".into(),
        existing_subfields: vec![],
        prefix: "command:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(r.is_some(), "command sub-field should have hover");
    let contents = r.unwrap().contents;
    assert!(
        contents.contains("Shell Command") || contents.contains("command"),
        "command hover should mention 'Shell Command' or 'command', got: {contents}"
    );
}

#[test]
fn hover_verb_subfield_unknown_falls_back_to_verb() {
    let ctx = CursorContext::VerbBlock {
        task_id: Some("step1".into()),
        verb: "infer".into(),
        existing_subfields: vec![],
        prefix: "unknown_field:".into(),
    };
    let r = hover("", 0, &ctx, None);
    assert!(
        r.is_some(),
        "unknown sub-field should fall back to verb-level hover"
    );
    let contents = r.unwrap().contents;
    assert!(
        contents.contains("LLM Generation"),
        "fallback hover should contain verb doc 'LLM Generation', got: {contents}"
    );
}
