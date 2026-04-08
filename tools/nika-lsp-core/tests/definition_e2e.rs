// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! End-to-end tests for the go-to-definition handler.
//!
//! Each test builds a realistic Nika workflow YAML and exercises the
//! definition handler. Tests fall into two categories:
//!
//! - **Full pipeline** (`detect_context` -> `definition`): for `with:` blocks
//!   and `{{template}}` expressions where the context detection feeds cleanly
//!   into the handler.
//!
//! - **Handler-level** (manual `CursorContext` -> `definition`): for
//!   `depends_on:` entries, where `detect_context` produces a prefix that
//!   includes the YAML list marker (`- task_name`). The handler is tested
//!   directly with the expected stripped prefix, matching real-world LSP
//!   behaviour where the prefix is cleaned before dispatch.

use nika_lsp_core::analysis::context::{detect_context, CursorContext};
use nika_lsp_core::handlers::definition::{definition, DefinitionResult};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run the full definition pipeline at a raw byte offset.
fn goto_def_at(text: &str, offset: u32) -> Option<DefinitionResult> {
    let ctx = detect_context(text, offset, None);
    definition(text, offset, &ctx)
}

/// Find the byte offset right after the first occurrence of `needle` in `text`.
fn offset_after(text: &str, needle: &str) -> u32 {
    let pos = text
        .find(needle)
        .unwrap_or_else(|| panic!("needle {needle:?} not found"));
    (pos + needle.len()) as u32
}

/// Build a DependsOn context with just the task name (no list marker).
fn depends_on_ctx(task_name: &str) -> CursorContext {
    CursorContext::DependsOn {
        task_id: None,
        existing_deps: vec![],
        prefix: task_name.to_string(),
    }
}

/// Assert that a `DefinitionResult` points at the `- id: <name>` declaration
/// and that the matched span actually contains the expected needle.
fn assert_jumps_to_task(text: &str, result: &DefinitionResult, task_id: &str) {
    let span = &text[result.offset as usize..result.end_offset as usize];
    assert!(
        span.contains(&format!("id: {task_id}"))
            || span.contains(&format!("id: \"{task_id}\""))
            || span.contains(&format!("id: '{task_id}'")),
        "expected span to reference task '{task_id}', got: {span:?}"
    );
    assert!(
        result.file.is_none(),
        "expected same-file jump (file == None)"
    );
}

// ===========================================================================
// 1. depends_on reference -> finds task
// ===========================================================================

#[test]
fn depends_on_reference_finds_task() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: fetch_data
    fetch:
      url: https://api.example.com/data
  - id: process
    depends_on:
      - fetch_data
    exec:
      command: python process.py
";
    let ctx = depends_on_ctx("fetch_data");
    let result = definition(yaml, 0, &ctx).expect("should resolve depends_on reference");
    assert_jumps_to_task(yaml, &result, "fetch_data");
}

#[test]
fn depends_on_middle_of_identifier() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step_alpha
    infer:
      prompt: do something
  - id: step_beta
    depends_on:
      - step_alpha
    infer:
      prompt: continue
";
    let ctx = depends_on_ctx("step_alpha");
    let result = definition(yaml, 0, &ctx).expect("should resolve depends_on ref");
    assert_jumps_to_task(yaml, &result, "step_alpha");
}

// ===========================================================================
// 2. with: block reference -> finds source task
// ===========================================================================

#[test]
fn with_block_reference_finds_source_task() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: gather
    fetch:
      url: https://example.com/api
  - id: summarize
    with:
      data: $gather
    infer:
      prompt: Summarize {{with.data}}
";
    // Cursor at end of "$gather" in the with: block
    let offset = offset_after(yaml, "$gather");
    let result = goto_def_at(yaml, offset).expect("should resolve with: reference");
    assert_jumps_to_task(yaml, &result, "gather");
}

#[test]
fn with_block_dollar_prefix_stripped() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: producer
    exec:
      command: echo hello
  - id: consumer
    with:
      result: $producer
    infer:
      prompt: Got {{with.result}}
";
    let offset = offset_after(yaml, "$producer");
    let result = goto_def_at(yaml, offset).expect("should strip $ and resolve");
    assert_jumps_to_task(yaml, &result, "producer");
}

// ===========================================================================
// 3. Template {{with.alias}} -> finds source
// ===========================================================================

#[test]
fn template_with_alias_finds_source() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: loader
    fetch:
      url: https://data.example.com
  - id: renderer
    with:
      payload: $loader
    infer:
      prompt: \"Render this: {{with.payload}}\"
";
    // Place cursor after the full alias inside {{with.payload}}, before }}.
    let tpl_pos = yaml.find("{{with.payload").expect("template must exist");
    let offset = (tpl_pos + "{{with.payload".len()) as u32;
    let ctx = detect_context(yaml, offset, None);
    let result = definition(yaml, offset, &ctx).expect("should resolve template reference");
    assert_jumps_to_task(yaml, &result, "loader");
}

#[test]
fn template_with_alias_at_start() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: source_task
    exec:
      command: echo data
  - id: consumer_task
    with:
      src: $source_task
    infer:
      prompt: \"Use {{with.src}} here\"
";
    let tpl_pos = yaml.find("{{with.src").expect("template must exist");
    let offset = (tpl_pos + "{{with.src".len()) as u32;
    let ctx = detect_context(yaml, offset, None);
    let result = definition(yaml, offset, &ctx).expect("should resolve short alias");
    assert_jumps_to_task(yaml, &result, "source_task");
}

// ===========================================================================
// 4. Unknown task -> None
// ===========================================================================

#[test]
fn unknown_task_returns_none() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: real_task
    infer:
      prompt: hello
  - id: another
    depends_on:
      - nonexistent_task
    infer:
      prompt: world
";
    let ctx = depends_on_ctx("nonexistent_task");
    let result = definition(yaml, 0, &ctx);
    assert!(
        result.is_none(),
        "unknown task reference should return None"
    );
}

#[test]
fn unknown_with_reference_returns_none() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: only_task
    with:
      data: $missing_task
    infer:
      prompt: \"{{with.data}}\"
";
    let offset = offset_after(yaml, "$missing_task");
    let result = goto_def_at(yaml, offset);
    assert!(
        result.is_none(),
        "with: ref to missing task should return None"
    );
}

#[test]
fn template_unknown_alias_returns_none() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: my_task
    infer:
      prompt: \"{{with.nonexistent}}\"
";
    let tpl_pos = yaml.find("{{with.nonexistent").expect("must exist");
    let offset = (tpl_pos + "{{with.nonexis".len()) as u32;
    let ctx = detect_context(yaml, offset, None);
    let result = definition(yaml, offset, &ctx);
    assert!(
        result.is_none(),
        "template ref to missing alias should return None"
    );
}

#[test]
fn cursor_on_irrelevant_context_returns_none() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      prompt: hello world
";
    // Cursor on the "prompt:" field -- VerbBlock context, not a reference
    let offset = offset_after(yaml, "prompt: hello");
    let result = goto_def_at(yaml, offset);
    assert!(result.is_none(), "non-reference context should return None");
}

// ===========================================================================
// 5. Self-reference -> finds self
// ===========================================================================

#[test]
fn self_reference_in_depends_on_finds_self() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: recursive
    depends_on:
      - recursive
    infer:
      prompt: loop
";
    let ctx = depends_on_ctx("recursive");
    let result = definition(yaml, 0, &ctx).expect("self-reference should resolve");
    assert_jumps_to_task(yaml, &result, "recursive");
}

#[test]
fn self_reference_in_with_finds_self() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: self_ref
    with:
      me: $self_ref
    infer:
      prompt: \"{{with.me}}\"
";
    let offset = offset_after(yaml, "$self_ref");
    let result = goto_def_at(yaml, offset).expect("with: self-ref should resolve");
    assert_jumps_to_task(yaml, &result, "self_ref");
}

// ===========================================================================
// 6. Quoted task IDs -> finds correctly
// ===========================================================================

#[test]
fn double_quoted_task_id_found_by_depends_on() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: \"quoted-task\"
    infer:
      prompt: hello
  - id: follower
    depends_on:
      - quoted-task
    infer:
      prompt: follow up
";
    let ctx = depends_on_ctx("quoted-task");
    let result = definition(yaml, 0, &ctx).expect("should find double-quoted task ID");
    assert_jumps_to_task(yaml, &result, "quoted-task");
}

#[test]
fn single_quoted_task_id_found_by_depends_on() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: 'single-quoted'
    infer:
      prompt: hello
  - id: follower
    depends_on:
      - single-quoted
    infer:
      prompt: follow up
";
    let ctx = depends_on_ctx("single-quoted");
    let result = definition(yaml, 0, &ctx).expect("should find single-quoted task ID");
    assert_jumps_to_task(yaml, &result, "single-quoted");
}

#[test]
fn quoted_task_found_via_with_reference() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: \"data-source\"
    fetch:
      url: https://api.example.com
  - id: processor
    with:
      input: $data-source
    infer:
      prompt: process {{with.input}}
";
    let offset = offset_after(yaml, "$data-source");
    let result = goto_def_at(yaml, offset).expect("should resolve with: to quoted task");
    assert_jumps_to_task(yaml, &result, "data-source");
}

// ===========================================================================
// 7. Multi-task workflow -> correct task found
// ===========================================================================

#[test]
fn multi_task_depends_on_first_task() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    exec:
      command: echo one
  - id: step2
    exec:
      command: echo two
  - id: step3
    exec:
      command: echo three
  - id: step4
    depends_on:
      - step1
    exec:
      command: echo four
";
    let ctx = depends_on_ctx("step1");
    let result = definition(yaml, 0, &ctx).expect("should find step1 among many tasks");
    assert_jumps_to_task(yaml, &result, "step1");
}

#[test]
fn multi_task_depends_on_middle_task() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: alpha
    exec:
      command: echo a
  - id: beta
    exec:
      command: echo b
  - id: gamma
    exec:
      command: echo c
  - id: delta
    depends_on:
      - beta
    exec:
      command: echo d
";
    let ctx = depends_on_ctx("beta");
    let result = definition(yaml, 0, &ctx).expect("should find beta (middle task)");
    assert_jumps_to_task(yaml, &result, "beta");
}

#[test]
fn multi_task_depends_on_last_task() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: first
    exec:
      command: echo 1
  - id: second
    exec:
      command: echo 2
  - id: third
    exec:
      command: echo 3
  - id: fourth
    depends_on:
      - third
    exec:
      command: echo 4
";
    let ctx = depends_on_ctx("third");
    let result = definition(yaml, 0, &ctx).expect("should find third (last before current)");
    assert_jumps_to_task(yaml, &result, "third");
}

#[test]
fn multi_task_with_reference_targets_correct_source() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: fetch_users
    fetch:
      url: https://api.example.com/users
  - id: fetch_posts
    fetch:
      url: https://api.example.com/posts
  - id: merge
    with:
      users: $fetch_users
      posts: $fetch_posts
    infer:
      prompt: Merge {{with.users}} and {{with.posts}}
";
    // "$fetch_posts" in with: block
    let offset_posts = offset_after(yaml, "$fetch_posts");
    let result = goto_def_at(yaml, offset_posts).expect("should resolve to fetch_posts");
    assert_jumps_to_task(yaml, &result, "fetch_posts");

    // "$fetch_users" in with: block
    let offset_users = offset_after(yaml, "$fetch_users");
    let result2 = goto_def_at(yaml, offset_users).expect("should resolve to fetch_users");
    assert_jumps_to_task(yaml, &result2, "fetch_users");
}

#[test]
fn multi_task_template_resolves_correct_binding() {
    // Each template is on its own line so detect_context's heuristic
    // (prefix must contain {{ without }}) works for each independently.
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: source_a
    exec:
      command: echo a
  - id: source_b
    exec:
      command: echo b
  - id: combiner
    with:
      a_data: $source_a
      b_data: $source_b
    infer:
      prompt: |
        Use {{with.a_data}}
        then {{with.b_data}}
";
    // Template {{with.a_data}} should resolve to source_a.
    let a_tpl_pos = yaml.find("{{with.a_data").expect("must exist");
    let offset_a = (a_tpl_pos + "{{with.a_data".len()) as u32;
    let ctx_a = detect_context(yaml, offset_a, None);
    let result_a = definition(yaml, offset_a, &ctx_a).expect("should resolve a_data to source_a");
    assert_jumps_to_task(yaml, &result_a, "source_a");

    // Template {{with.b_data}} should resolve to source_b.
    let b_tpl_pos = yaml.find("{{with.b_data").expect("must exist");
    let offset_b = (b_tpl_pos + "{{with.b_data".len()) as u32;
    let ctx_b = detect_context(yaml, offset_b, None);
    let result_b = definition(yaml, offset_b, &ctx_b).expect("should resolve b_data to source_b");
    assert_jumps_to_task(yaml, &result_b, "source_b");
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn empty_depends_on_returns_none() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      prompt: hi
  - id: step2
    depends_on:
      -
    infer:
      prompt: hello
";
    let ctx = depends_on_ctx("");
    let result = definition(yaml, 0, &ctx);
    assert!(
        result.is_none(),
        "empty depends_on entry should return None"
    );
}

#[test]
fn hyphenated_task_id_in_depends_on() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: my-complex-task-name
    infer:
      prompt: hello
  - id: consumer
    depends_on:
      - my-complex-task-name
    infer:
      prompt: world
";
    let ctx = depends_on_ctx("my-complex-task-name");
    let result = definition(yaml, 0, &ctx).expect("should find hyphenated task ID");
    assert_jumps_to_task(yaml, &result, "my-complex-task-name");
}

#[test]
fn underscored_task_id_in_depends_on() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: my_underscored_task
    infer:
      prompt: hello
  - id: consumer
    depends_on:
      - my_underscored_task
    infer:
      prompt: world
";
    let ctx = depends_on_ctx("my_underscored_task");
    let result = definition(yaml, 0, &ctx).expect("should find underscored task ID");
    assert_jumps_to_task(yaml, &result, "my_underscored_task");
}

#[test]
fn definition_result_offsets_are_correct() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: target
    infer:
      prompt: hello
  - id: jumper
    depends_on:
      - target
    infer:
      prompt: world
";
    let ctx = depends_on_ctx("target");
    let result = definition(yaml, 0, &ctx).expect("should resolve");
    // The span should match the "- id: target" declaration exactly.
    let span_text = &yaml[result.offset as usize..result.end_offset as usize];
    assert_eq!(
        span_text, "- id: target",
        "span should be exactly the task declaration"
    );
}

#[test]
fn realistic_workflow_full_pipeline() {
    let yaml = "\
schema: nika/workflow@0.12
name: data-pipeline
mcp:
  novanet:
    command: cargo run
tasks:
  - id: fetch_raw
    fetch:
      url: https://api.example.com/raw
  - id: transform
    depends_on:
      - fetch_raw
    with:
      raw: $fetch_raw
    exec:
      command: python transform.py
  - id: enrich
    depends_on:
      - transform
    with:
      transformed: $transform
    invoke:
      mcp: novanet
      tool: enrich
      params:
        data: \"{{with.transformed}}\"
  - id: summarize
    depends_on:
      - enrich
    with:
      enriched: $enrich
    infer:
      prompt: \"Summarize: {{with.enriched}}\"
";
    // depends_on tests (handler-level)
    let ctx1 = depends_on_ctx("fetch_raw");
    let r1 = definition(yaml, 0, &ctx1).expect("transform -> fetch_raw");
    assert_jumps_to_task(yaml, &r1, "fetch_raw");

    let ctx2 = depends_on_ctx("transform");
    let r2 = definition(yaml, 0, &ctx2).expect("enrich -> transform");
    assert_jumps_to_task(yaml, &r2, "transform");

    let ctx3 = depends_on_ctx("enrich");
    let r3 = definition(yaml, 0, &ctx3).expect("summarize -> enrich");
    assert_jumps_to_task(yaml, &r3, "enrich");

    // with: block test (full pipeline)
    let with_offset = offset_after(yaml, "$enrich");
    let r4 = goto_def_at(yaml, with_offset).expect("with: $enrich");
    assert_jumps_to_task(yaml, &r4, "enrich");

    // Template test (full pipeline) -- cursor after full alias, before }}
    let tpl_pos = yaml.find("{{with.enriched").expect("template must exist");
    let tpl_offset = (tpl_pos + "{{with.enriched".len()) as u32;
    let ctx = detect_context(yaml, tpl_offset, None);
    let r5 = definition(yaml, tpl_offset, &ctx).expect("template -> enrich");
    assert_jumps_to_task(yaml, &r5, "enrich");
}

// ===========================================================================
// 9. agent: from: -> jumps to agents: block definition
// ===========================================================================

#[test]
fn agent_from_jumps_to_agents_block() {
    let yaml = "\
schema: nika/workflow@0.12
agents:
  researcher:
    system: \"You are a researcher\"
tasks:
  - id: step1
    agent:
      from: researcher
      prompt: \"Research this topic\"
";
    // Place offset on the `from: researcher` line so extract_field_value finds it.
    let from_pos = yaml
        .find("from: researcher")
        .expect("from: line must exist");
    let offset = (from_pos + "from: res".len()) as u32;
    let ctx = CursorContext::VerbBlock {
        task_id: Some("step1".to_string()),
        verb: "agent".to_string(),
        existing_subfields: vec![],
        prefix: "from:".to_string(),
    };
    let result = definition(yaml, offset, &ctx).expect("should jump to agent definition");
    let span = &yaml[result.offset as usize..result.end_offset as usize];
    assert!(
        span.contains("researcher"),
        "expected span to contain 'researcher', got: {span:?}"
    );
    assert!(
        result.file.is_none(),
        "expected same-file jump (file == None)"
    );
}

// ===========================================================================
// 10. mcp config -> jumps to server definition
// ===========================================================================

#[test]
fn mcp_config_jumps_to_server() {
    let yaml = "\
schema: nika/workflow@0.12
mcp:
  novanet:
    command: node
tasks:
  - id: step1
    invoke:
      mcp: novanet
      tool: search
";
    let ctx = CursorContext::McpConfig {
        server_name: Some("novanet".to_string()),
        prefix: String::new(),
    };
    let result = definition(yaml, 0, &ctx).expect("should find mcp server definition");
    let span = &yaml[result.offset as usize..result.end_offset as usize];
    assert!(
        span.contains("novanet"),
        "expected span to contain 'novanet', got: {span:?}"
    );
    assert!(
        result.file.is_none(),
        "expected same-file jump (file == None)"
    );
}

// ===========================================================================
// 11. {{inputs.topic}} template -> jumps to inputs: root key
// ===========================================================================

#[test]
fn template_inputs_jumps_to_inputs_block() {
    let yaml = "\
schema: nika/workflow@0.12
inputs:
  topic:
    type: string
    default: AI
tasks:
  - id: step1
    infer:
      prompt: \"Write about {{inputs.topic}}\"
";
    let ctx = CursorContext::Template {
        task_id: Some("step1".to_string()),
        available_bindings: vec![],
        partial_expr: "inputs.topic".to_string(),
        in_transform_chain: false,
    };
    let result = definition(yaml, 0, &ctx).expect("should jump to inputs: root key");
    let span = &yaml[result.offset as usize..result.end_offset as usize];
    assert!(
        span.starts_with("inputs:"),
        "expected span to start with 'inputs:', got: {span:?}"
    );
    assert!(
        result.file.is_none(),
        "expected same-file jump (file == None)"
    );
}

// ===========================================================================
// 12. {{context.files.brand}} template -> jumps to context: root key
// ===========================================================================

#[test]
fn template_context_jumps_to_context_block() {
    let yaml = "\
schema: nika/workflow@0.12
context:
  files:
    brand: ./brand.md
tasks:
  - id: step1
    infer:
      prompt: \"Use {{context.files.brand}} as reference\"
";
    let ctx = CursorContext::Template {
        task_id: Some("step1".to_string()),
        available_bindings: vec![],
        partial_expr: "context.files.brand".to_string(),
        in_transform_chain: false,
    };
    let result = definition(yaml, 0, &ctx).expect("should jump to context: root key");
    let span = &yaml[result.offset as usize..result.end_offset as usize];
    assert!(
        span.starts_with("context:"),
        "expected span to start with 'context:', got: {span:?}"
    );
    assert!(
        result.file.is_none(),
        "expected same-file jump (file == None)"
    );
}

// ===========================================================================
// 13. depends_on array prefix -> finds last task ID
// ===========================================================================

#[test]
fn depends_on_array_finds_last_id() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    exec:
      command: echo one
  - id: step2
    exec:
      command: echo two
  - id: step3
    depends_on: [step1, step2]
    infer:
      prompt: combine results
";
    let ctx = CursorContext::DependsOn {
        task_id: Some("step3".to_string()),
        existing_deps: vec![],
        prefix: "[step1, step2".to_string(),
    };
    let result = definition(yaml, 0, &ctx).expect("should find step2 from array prefix");
    assert_jumps_to_task(yaml, &result, "step2");
}

// ===========================================================================
// 14. with: $ref -> traces through binding to task definition
// ===========================================================================

#[test]
fn with_ref_traces_through_binding() {
    let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    exec:
      command: echo hello
  - id: step2
    with:
      result: $step1
    infer:
      prompt: \"Got {{with.result}}\"
";
    let ctx = CursorContext::WithBlock {
        task_id: Some("step2".to_string()),
        alias: Some("result".to_string()),
        partial_ref: "$step1".to_string(),
    };
    let result = definition(yaml, 0, &ctx).expect("should trace $step1 to task definition");
    assert_jumps_to_task(yaml, &result, "step1");
}
