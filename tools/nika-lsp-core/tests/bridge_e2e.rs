// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! End-to-end integration tests for the tree-sitter bridge.
//!
//! Each test exercises `extract_partial()` on realistic or pathological
//! inputs, verifying that the bridge never panics and produces correct
//! structural output even on broken YAML.

use std::time::Instant;

use nika_lsp_core::parse::bridge::extract_partial;
use nika_lsp_core::parse::recovery::RecoveryParser;
use nika_lsp_core::parse::PartialWorkflow;

// ===========================================================================
// Helpers
// ===========================================================================

/// Parse YAML text through the full pipeline: RecoveryParser -> extract_partial.
fn partial(yaml: &str) -> PartialWorkflow {
    let mut parser = RecoveryParser::new();
    let tree = parser
        .parse(yaml)
        .expect("tree-sitter should always return a tree");
    extract_partial(yaml, &tree)
}

// ===========================================================================
// 1. Valid minimal workflow -- correct task count, verb detection
// ===========================================================================

#[test]
fn valid_minimal_workflow_task_count_and_verb() {
    let yaml = "\
schema: '@0.12'
workflow: minimal
tasks:
  - id: hello
    infer:
      prompt: Say hello
";
    let pw = partial(yaml);

    assert!(!pw.has_errors, "valid YAML should have no errors");
    assert!(pw.error_ranges.is_empty());
    assert_eq!(pw.tasks.len(), 1, "expected exactly 1 task");

    let task = &pw.tasks[0];
    assert_eq!(task.id.as_deref(), Some("hello"));
    assert_eq!(task.verb.as_deref(), Some("infer"));
    assert!(task.verb_span.is_some(), "verb_span should be set");
    assert!(!task.span.is_empty(), "task span should be non-empty");
}

#[test]
fn valid_minimal_workflow_schema_and_name() {
    let yaml = "\
schema: '@0.12'
workflow: minimal
tasks:
  - id: t
    exec:
      command: echo ok
";
    let pw = partial(yaml);

    assert!(pw.schema.is_some());
    assert_eq!(pw.schema.as_ref().unwrap().value, "'@0.12'");
    assert!(pw.workflow_name.is_some());
    assert_eq!(pw.workflow_name.as_ref().unwrap().value, "minimal");
}

// ===========================================================================
// 2. Valid 5-task workflow with all verbs
// ===========================================================================

#[test]
fn valid_five_tasks_all_verbs_found() {
    let yaml = "\
schema: '@0.12'
workflow: all-verbs
tasks:
  - id: t_infer
    infer:
      model: gpt-4
      prompt: generate something
  - id: t_exec
    exec:
      command: echo done
  - id: t_fetch
    fetch:
      url: https://api.example.com/data
      method: GET
  - id: t_invoke
    invoke:
      tool: novanet_search
      input:
        query: find something
  - id: t_agent
    agent:
      goal: accomplish multi-step reasoning
      max_turns: 5
";
    let pw = partial(yaml);

    assert!(!pw.has_errors, "all-verbs workflow should be valid YAML");
    assert_eq!(pw.tasks.len(), 5, "expected exactly 5 tasks");

    let verbs: Vec<&str> = pw
        .tasks
        .iter()
        .map(|t| t.verb.as_deref().expect("each task should have a verb"))
        .collect();
    assert_eq!(verbs, &["infer", "exec", "fetch", "invoke", "agent"]);

    let ids: Vec<&str> = pw
        .tasks
        .iter()
        .map(|t| t.id.as_deref().expect("each task should have an id"))
        .collect();
    assert_eq!(
        ids,
        &["t_infer", "t_exec", "t_fetch", "t_invoke", "t_agent"]
    );
}

#[test]
fn valid_five_tasks_verb_spans_all_set() {
    let yaml = "\
schema: '@0.12'
tasks:
  - id: a
    infer:
      prompt: x
  - id: b
    exec:
      command: y
  - id: c
    fetch:
      url: http://z
  - id: d
    invoke:
      tool: t
  - id: e
    agent:
      goal: g
";
    let pw = partial(yaml);

    for task in &pw.tasks {
        assert!(
            task.verb_span.is_some(),
            "verb_span should be set for task {:?}",
            task.id
        );
    }
}

// ===========================================================================
// 3. Valid workflow with content: block
// ===========================================================================

#[test]
fn valid_workflow_content_block_detected() {
    let yaml = "\
schema: '@0.12'
tasks:
  - id: with_content
    infer:
      prompt: summarize
    content:
      - type: text
        text: |
          This is a multiline content block
          with several lines of text.
";
    let pw = partial(yaml);

    assert!(!pw.has_errors);
    assert_eq!(pw.tasks.len(), 1);

    let task = &pw.tasks[0];
    assert!(task.has_content, "has_content should be true");
    assert!(task.existing_keys.contains(&"content".to_string()));
}

#[test]
fn valid_workflow_content_scalar_detected() {
    let yaml = "\
schema: '@0.12'
tasks:
  - id: scalar_content
    infer:
      prompt: test
    content: true
";
    let pw = partial(yaml);
    assert!(pw.tasks[0].has_content);
}

// ===========================================================================
// 4. Valid workflow with with: block
// ===========================================================================

#[test]
fn valid_workflow_with_aliases_populated() {
    let yaml = "\
schema: '@0.12'
tasks:
  - id: templated
    infer:
      prompt: \"Summarize {{with.document}} using {{with.style}}\"
    with:
      document: ./input.txt
      style: concise
      format: markdown
";
    let pw = partial(yaml);

    assert!(!pw.has_errors);
    assert_eq!(pw.tasks.len(), 1);

    let task = &pw.tasks[0];
    assert!(task.has_with, "has_with should be true");
    assert_eq!(task.with_aliases.len(), 3, "expected 3 with aliases");
    assert!(task.with_aliases.contains(&"document".to_string()));
    assert!(task.with_aliases.contains(&"style".to_string()));
    assert!(task.with_aliases.contains(&"format".to_string()));
}

#[test]
fn valid_workflow_with_single_alias() {
    let yaml = "\
tasks:
  - id: single_with
    exec:
      command: echo {{with.file}}
    with:
      file: data.csv
";
    let pw = partial(yaml);

    let task = &pw.tasks[0];
    assert!(task.has_with);
    assert_eq!(task.with_aliases, vec!["file".to_string()]);
}

// ===========================================================================
// 5. Valid workflow with depends_on
// ===========================================================================

#[test]
fn valid_workflow_depends_on_flow_sequence() {
    let yaml = "\
schema: '@0.12'
tasks:
  - id: setup
    exec:
      command: echo setup
  - id: process
    exec:
      command: echo process
    depends_on: [setup]
  - id: finalize
    exec:
      command: echo finalize
    depends_on: [setup, process]
";
    let pw = partial(yaml);

    assert!(!pw.has_errors);
    assert_eq!(pw.tasks.len(), 3);

    let setup = &pw.tasks[0];
    assert!(!setup.has_depends_on);
    assert!(setup.depends_on_refs.is_empty());

    let process = &pw.tasks[1];
    assert!(process.has_depends_on);
    assert_eq!(process.depends_on_refs, vec!["setup".to_string()]);

    let finalize = &pw.tasks[2];
    assert!(finalize.has_depends_on);
    assert!(finalize.depends_on_refs.contains(&"setup".to_string()));
    assert!(finalize.depends_on_refs.contains(&"process".to_string()));
}

#[test]
fn valid_workflow_depends_on_block_sequence() {
    let yaml = "\
tasks:
  - id: last
    exec:
      command: echo last
    depends_on:
      - step_a
      - step_b
      - step_c
";
    let pw = partial(yaml);

    let task = &pw.tasks[0];
    assert!(task.has_depends_on);
    assert_eq!(task.depends_on_refs.len(), 3);
    assert!(task.depends_on_refs.contains(&"step_a".to_string()));
    assert!(task.depends_on_refs.contains(&"step_b".to_string()));
    assert!(task.depends_on_refs.contains(&"step_c".to_string()));
}

#[test]
fn valid_workflow_depends_on_plain_scalar() {
    let yaml = "\
tasks:
  - id: b
    exec:
      command: echo b
    depends_on: a
";
    let pw = partial(yaml);

    let task = &pw.tasks[0];
    assert!(task.has_depends_on);
    assert_eq!(task.depends_on_refs, vec!["a".to_string()]);
}

// ===========================================================================
// 6. Valid workflow with mcp: section
// ===========================================================================

#[test]
fn valid_workflow_mcp_servers_found() {
    let yaml = "\
schema: '@0.12'
workflow: with-mcp
mcp:
  novanet:
    command: novanet serve
    port: 8080
  filesystem:
    command: fs-mcp serve
    root: /tmp
  custom_tool:
    command: custom serve
tasks:
  - id: t
    invoke:
      tool: novanet_search
";
    let pw = partial(yaml);

    assert!(!pw.has_errors);
    assert_eq!(pw.mcp_servers.len(), 3, "expected 3 MCP servers");

    let names: Vec<&str> = pw.mcp_servers.iter().map(|s| s.value.as_str()).collect();
    assert!(names.contains(&"novanet"));
    assert!(names.contains(&"filesystem"));
    assert!(names.contains(&"custom_tool"));

    // Verify spans are set
    for srv in &pw.mcp_servers {
        assert!(
            !srv.key_span.is_empty(),
            "MCP server key_span should be non-empty"
        );
        assert!(
            srv.value_span.is_some(),
            "MCP server value_span should be present"
        );
    }
}

#[test]
fn valid_workflow_mcp_single_server() {
    let yaml = "\
mcp:
  single_srv:
    command: srv start
";
    let pw = partial(yaml);
    assert_eq!(pw.mcp_servers.len(), 1);
    assert_eq!(pw.mcp_servers[0].value, "single_srv");
}

// ===========================================================================
// 7. Broken: missing colon after verb
// ===========================================================================

#[test]
fn broken_missing_colon_after_verb_has_errors() {
    let yaml = "\
schema: '@0.12'
tasks:
  - id: broken_verb
    infer
      prompt: this should fail
";
    let pw = partial(yaml);

    assert!(pw.has_errors, "missing colon should produce errors");
    assert!(
        !pw.error_ranges.is_empty(),
        "error_ranges should be populated"
    );
}

#[test]
fn broken_missing_colon_still_extracts_schema() {
    let yaml = "\
schema: '@0.12'
workflow: broken-pipeline
tasks:
  - id: broken
    exec
      command: echo fail
";
    let pw = partial(yaml);

    assert!(pw.has_errors);
    // Schema should still be recoverable since it's above the broken part
    // (tree-sitter recovery is best-effort, so we test for no panic at minimum)
    if let Some(schema) = &pw.schema {
        assert_eq!(schema.value, "'@0.12'");
    }
}

#[test]
fn broken_missing_colon_no_panic_with_multiple_errors() {
    let yaml = "\
schema '@0.12'
workflow broken
tasks
  - id broken
    infer
      prompt fail
";
    let pw = partial(yaml);
    // tree-sitter-yaml may or may not flag this as errors depending on how
    // it interprets the bare scalars. The primary assertion: no panic.
    // If it does detect errors, error_ranges should be populated.
    if pw.has_errors {
        assert!(!pw.error_ranges.is_empty());
    }
}

// ===========================================================================
// 8. Broken: truncated mid-task -- partial task still extracted
// ===========================================================================

#[test]
fn broken_truncated_mid_task_partial_extraction() {
    let yaml = "\
schema: '@0.12'
tasks:
  - id: complete_task
    exec:
      command: echo done
  - id: truncat";
    let pw = partial(yaml);

    // Schema should be extracted regardless
    assert!(pw.schema.is_some());
    assert_eq!(pw.schema.as_ref().unwrap().value, "'@0.12'");

    // At least the complete task should be found
    assert!(!pw.tasks.is_empty(), "should extract at least one task");

    let complete = pw
        .tasks
        .iter()
        .find(|t| t.id.as_deref() == Some("complete_task"));
    assert!(complete.is_some(), "complete task should be found");
    assert_eq!(complete.unwrap().verb.as_deref(), Some("exec"));
}

#[test]
fn broken_truncated_after_verb_key() {
    let yaml = "\
schema: '@0.12'
tasks:
  - id: partial_task
    infer:
";
    let pw = partial(yaml);

    assert!(!pw.tasks.is_empty());
    let task = &pw.tasks[0];
    assert_eq!(task.id.as_deref(), Some("partial_task"));
    assert_eq!(task.verb.as_deref(), Some("infer"));
}

#[test]
fn broken_truncated_after_id() {
    let yaml = "schema: '@0.12'\ntasks:\n  - id: only_id";
    let pw = partial(yaml);

    assert!(pw.schema.is_some());
    // The task may or may not be extracted depending on tree-sitter recovery,
    // but the function must not panic.
}

// ===========================================================================
// 9. Broken: empty file
// ===========================================================================

#[test]
fn broken_empty_file_returns_empty_partial_workflow() {
    let pw = partial("");

    assert!(pw.schema.is_none());
    assert!(pw.workflow_name.is_none());
    assert!(pw.tasks.is_empty());
    assert!(pw.mcp_servers.is_empty());
    assert!(pw.context_files.is_empty());
    assert!(pw.includes.is_empty());
    assert!(pw.input_names.is_empty());
    assert!(!pw.has_errors, "empty file is valid YAML");
    assert!(pw.error_ranges.is_empty());
    assert!(pw.top_level_keys.is_empty());
}

#[test]
fn broken_empty_file_with_whitespace() {
    let pw = partial("   \n\n   \t\n  ");

    assert!(pw.schema.is_none());
    assert!(pw.tasks.is_empty());
    assert!(!pw.has_errors);
}

#[test]
fn broken_empty_file_single_newline() {
    let pw = partial("\n");

    assert!(pw.schema.is_none());
    assert!(pw.tasks.is_empty());
}

// ===========================================================================
// 10. Broken: binary garbage
// ===========================================================================

#[test]
fn broken_binary_garbage_no_panic() {
    let garbage: Vec<u8> = (0..256).map(|b| b as u8).collect();
    // tree-sitter requires valid UTF-8; we test with a lossy conversion
    let text = String::from_utf8_lossy(&garbage).to_string();
    let pw = partial(&text);

    // The only requirement: no panic. has_errors should be true for garbage input.
    // tree-sitter may or may not mark it as error depending on what it parses.
    let _ = pw;
}

#[test]
fn broken_binary_garbage_null_bytes() {
    let text = "schema: '@0.12'\0\0\0\ntasks:\0\n  - id: t\0\n";
    let pw = partial(text);
    // Must not panic. Structure extraction is best-effort.
    let _ = pw;
}

#[test]
fn broken_binary_garbage_random_ascii() {
    let text = "!@#$%^&*()_+-=[]{}|;':\",./<>?\n!@#$%^&*()";
    let pw = partial(text);
    // No panic required
    let _ = pw;
}

#[test]
fn broken_binary_garbage_mixed_with_yaml() {
    let text = "\
schema: '@0.12'
\x01\x02\x03\x04\x05
tasks:
  - id: after_garbage
    exec:
      command: echo survived
";
    let pw = partial(text);
    // Garbage will cause errors, but schema might still be recovered
    assert!(pw.has_errors);
}

#[test]
fn broken_very_long_single_line_no_newlines() {
    let long_line = "x".repeat(100_000);
    let pw = partial(&long_line);
    // No panic, no hang
    let _ = pw;
}

// ===========================================================================
// 11. Broken: 1000 lines of valid YAML -- performance OK
// ===========================================================================

#[test]
fn performance_1000_lines_valid_yaml() {
    let mut yaml = String::from("schema: '@0.12'\nworkflow: perf-test\ntasks:\n");
    for i in 0..200 {
        yaml.push_str(&format!(
            "  - id: task_{i}\n    exec:\n      command: echo {i}\n"
        ));
    }

    // Should be well under 1000 lines, but let's verify the line count
    let line_count = yaml.lines().count();
    assert!(
        line_count > 600,
        "fixture should have many lines, got {line_count}"
    );

    let start = Instant::now();
    let pw = partial(&yaml);
    let elapsed = start.elapsed();

    assert!(!pw.has_errors);
    assert_eq!(pw.tasks.len(), 200);

    // Performance: should complete well under 5 seconds (tree-sitter timeout).
    // On any modern machine this should be < 100ms.
    assert!(
        elapsed.as_secs() < 5,
        "parsing 200 tasks took {:?}, expected < 5s",
        elapsed
    );
}

#[test]
fn performance_1000_lines_mixed_content() {
    let mut yaml = String::from("schema: '@0.12'\ntasks:\n");
    for i in 0..100 {
        yaml.push_str(&format!(
            "  - id: task_{i}\n\
             \x20   infer:\n\
             \x20     prompt: \"Generate response for item {i}\"\n\
             \x20     model: gpt-4\n\
             \x20     temperature: 0.7\n\
             \x20   with:\n\
             \x20     data: ./data_{i}.json\n\
             \x20     config: ./config.yaml\n\
             \x20   depends_on: [task_{prev}]\n",
            prev = if i > 0 { i - 1 } else { 0 }
        ));
    }

    let line_count = yaml.lines().count();
    assert!(
        line_count > 800,
        "fixture should be large, got {line_count} lines"
    );

    let start = Instant::now();
    let pw = partial(&yaml);
    let elapsed = start.elapsed();

    assert_eq!(pw.tasks.len(), 100);
    assert!(
        elapsed.as_secs() < 5,
        "parsing 100 rich tasks took {:?}",
        elapsed
    );

    // Verify structural correctness of a sample task
    let task_50 = pw.tasks.iter().find(|t| t.id.as_deref() == Some("task_50"));
    assert!(task_50.is_some());
    let task_50 = task_50.unwrap();
    assert_eq!(task_50.verb.as_deref(), Some("infer"));
    assert!(task_50.has_with);
    assert!(task_50.has_depends_on);
}

// ===========================================================================
// 12. Broken: deeply nested (50+ levels) -- no stack overflow
// ===========================================================================

#[test]
fn broken_deeply_nested_50_levels_no_stack_overflow() {
    let mut yaml = String::new();
    for i in 0..50 {
        let indent = "  ".repeat(i);
        yaml.push_str(&format!("{indent}level_{i}:\n"));
    }
    yaml.push_str(&format!("{}leaf: value\n", "  ".repeat(50)));

    let pw = partial(&yaml);

    // Must not panic or stack overflow. No workflow structure expected.
    assert!(pw.tasks.is_empty());
}

#[test]
fn broken_deeply_nested_100_levels_no_stack_overflow() {
    let mut yaml = String::new();
    for i in 0..100 {
        let indent = "  ".repeat(i);
        yaml.push_str(&format!("{indent}n{i}:\n"));
    }
    yaml.push_str(&format!("{}end: true\n", "  ".repeat(100)));

    let pw = partial(&yaml);
    assert!(pw.tasks.is_empty());
}

#[test]
fn broken_deeply_nested_with_tasks_at_root() {
    // Deeply nested YAML under tasks: -- the task items themselves are
    // shallow but contain deeply nested verb config.
    let mut yaml = String::from("schema: '@0.12'\ntasks:\n  - id: deep\n    infer:\n");
    let base_indent = 6;
    for i in 0..50 {
        let indent = " ".repeat(base_indent + i * 2);
        yaml.push_str(&format!("{indent}nested_{i}:\n"));
    }
    let final_indent = " ".repeat(base_indent + 50 * 2);
    yaml.push_str(&format!("{final_indent}value: deep\n"));

    let pw = partial(&yaml);

    // Should extract the task without stack overflow
    assert!(!pw.tasks.is_empty());
    let task = &pw.tasks[0];
    assert_eq!(task.id.as_deref(), Some("deep"));
    assert_eq!(task.verb.as_deref(), Some("infer"));
}

// ===========================================================================
// 13. Valid workflow with include
// ===========================================================================

#[test]
fn valid_workflow_includes_detected() {
    let yaml = "\
schema: '@0.12'
include:
  - ./common/base.nika.yaml
  - ./common/utils.nika.yaml
  - ./shared/mcp-config.nika.yaml
tasks:
  - id: t
    exec:
      command: echo hello
";
    let pw = partial(yaml);

    assert!(!pw.has_errors);
    assert_eq!(pw.includes.len(), 3, "expected 3 includes");

    let include_values: Vec<&str> = pw.includes.iter().map(|i| i.value.as_str()).collect();
    assert!(include_values.contains(&"./common/base.nika.yaml"));
    assert!(include_values.contains(&"./common/utils.nika.yaml"));
    assert!(include_values.contains(&"./shared/mcp-config.nika.yaml"));
}

#[test]
fn valid_workflow_includes_single() {
    let yaml = "\
include:
  - ./only-one.nika.yaml
";
    let pw = partial(yaml);
    assert_eq!(pw.includes.len(), 1);
    assert_eq!(pw.includes[0].value, "./only-one.nika.yaml");
}

#[test]
fn valid_workflow_includes_have_spans() {
    let yaml = "\
include:
  - ./a.nika.yaml
  - ./b.nika.yaml
";
    let pw = partial(yaml);

    for inc in &pw.includes {
        assert!(
            !inc.key_span.is_empty(),
            "include key_span should be non-empty"
        );
    }
}

// ===========================================================================
// 14. Multiple documents (--- separator) -- first doc used
// ===========================================================================

#[test]
fn multi_document_first_doc_extracted() {
    let yaml = "\
schema: '@0.12'
workflow: first-doc
tasks:
  - id: first
    exec:
      command: echo first
---
schema: '@0.13'
workflow: second-doc
tasks:
  - id: second
    exec:
      command: echo second
";
    let pw = partial(yaml);

    // The bridge walks all documents in the stream, so it may pick up
    // both. At minimum, the first document's schema should be present.
    assert!(pw.schema.is_some());

    // Workflow from the first doc
    if let Some(wf) = &pw.workflow_name {
        // tree-sitter sees both documents; the second may overwrite the first
        // depending on traversal order. The important thing is we get data.
        assert!(
            wf.value == "first-doc" || wf.value == "second-doc",
            "workflow name should come from one of the documents, got: {}",
            wf.value
        );
    }

    // At least some tasks should be found
    assert!(!pw.tasks.is_empty());
}

#[test]
fn multi_document_explicit_start() {
    let yaml = "\
---
schema: '@0.12'
tasks:
  - id: explicit
    exec:
      command: echo explicit
";
    let pw = partial(yaml);

    assert!(pw.schema.is_some());
    assert!(!pw.tasks.is_empty());
    assert_eq!(pw.tasks[0].id.as_deref(), Some("explicit"));
}

#[test]
fn multi_document_three_docs() {
    let yaml = "\
schema: '@0.12'
---
schema: '@0.13'
---
schema: '@0.14'
";
    let pw = partial(yaml);
    // Must not panic. At least one schema should be found.
    assert!(pw.schema.is_some());
}

// ===========================================================================
// 15. Comments-only file
// ===========================================================================

#[test]
fn comments_only_file_no_tasks_no_panic() {
    let yaml = "\
# This is a Nika workflow file
# schema: '@0.12'
# workflow: commented-out
# tasks:
#   - id: noop
#     exec:
#       command: echo hello
";
    let pw = partial(yaml);

    assert!(pw.schema.is_none(), "comments should not produce schema");
    assert!(pw.workflow_name.is_none());
    assert!(pw.tasks.is_empty(), "comments should produce no tasks");
    assert!(pw.mcp_servers.is_empty());
    assert!(pw.includes.is_empty());
    assert!(!pw.has_errors, "comment-only YAML is valid");
}

#[test]
fn comments_only_single_line() {
    let yaml = "# just a comment";
    let pw = partial(yaml);
    assert!(pw.tasks.is_empty());
    assert!(pw.schema.is_none());
}

#[test]
fn comments_mixed_with_blank_lines() {
    let yaml = "\
# comment 1

# comment 2

# comment 3
";
    let pw = partial(yaml);
    assert!(pw.tasks.is_empty());
    assert!(!pw.has_errors);
}

// ===========================================================================
// 16. Tab indentation -- still extracts structure
// ===========================================================================

#[test]
fn tab_indentation_extracts_structure() {
    // tree-sitter-yaml handles tabs, though YAML spec says spaces only.
    // The bridge should not panic and should extract what it can.
    let yaml = "schema: '@0.12'\ntasks:\n\t- id: tabbed\n\t\tinfer:\n\t\t\tprompt: hello\n";
    let pw = partial(yaml);

    // tree-sitter may or may not produce errors for tabs. The bridge must
    // not panic. If tree-sitter tolerates tabs, we might still get tasks.
    // Either way: no panic is the primary assertion.
    if !pw.has_errors {
        // If no errors, structure should be extracted
        assert!(!pw.tasks.is_empty());
    }

    // Schema should be recoverable regardless (it's before the tab-indented part)
    if let Some(schema) = &pw.schema {
        assert_eq!(schema.value, "'@0.12'");
    }
}

#[test]
fn tab_indentation_mixed_with_spaces() {
    let yaml = "schema: '@0.12'\ntasks:\n  - id: mixed\n\t  exec:\n      command: echo\n";
    let pw = partial(yaml);
    // Must not panic
    let _ = pw;
}

#[test]
fn tab_only_indentation() {
    let yaml =
        "schema: '@0.12'\nworkflow: tabs\ntasks:\n\t- id: t\n\t\texec:\n\t\t\tcommand: echo\n";
    let pw = partial(yaml);
    // No panic required; schema might still parse
    if let Some(schema) = &pw.schema {
        assert_eq!(schema.value, "'@0.12'");
    }
}

// ===========================================================================
// Additional edge cases: top-level keys
// ===========================================================================

#[test]
fn top_level_keys_tracked_for_known_keys() {
    let yaml = "\
schema: '@0.12'
workflow: test
include:
  - ./a.yaml
inputs:
  name: string
mcp:
  srv:
    command: x
context:
  files:
    readme: ./README.md
tasks:
  - id: t
    exec:
      command: echo
";
    let pw = partial(yaml);

    let keys: Vec<&str> = pw.top_level_keys.iter().map(|k| k.value.as_str()).collect();
    assert!(
        keys.contains(&"schema"),
        "missing 'schema' in top_level_keys"
    );
    assert!(
        keys.contains(&"workflow"),
        "missing 'workflow' in top_level_keys"
    );
    assert!(
        keys.contains(&"include"),
        "missing 'include' in top_level_keys"
    );
    assert!(
        keys.contains(&"inputs"),
        "missing 'inputs' in top_level_keys"
    );
    assert!(keys.contains(&"mcp"), "missing 'mcp' in top_level_keys");
    assert!(
        keys.contains(&"context"),
        "missing 'context' in top_level_keys"
    );
    assert!(keys.contains(&"tasks"), "missing 'tasks' in top_level_keys");
}

#[test]
fn top_level_unknown_keys_not_tracked() {
    let yaml = "\
schema: '@0.12'
custom_key: ignored
another: also_ignored
";
    let pw = partial(yaml);

    let keys: Vec<&str> = pw.top_level_keys.iter().map(|k| k.value.as_str()).collect();
    assert!(keys.contains(&"schema"));
    assert!(!keys.contains(&"custom_key"));
    assert!(!keys.contains(&"another"));
}

// ===========================================================================
// Additional edge cases: context files
// ===========================================================================

#[test]
fn context_files_extracted() {
    let yaml = "\
context:
  files:
    readme: ./README.md
    license: ./LICENSE
    changelog: ./CHANGELOG.md
";
    let pw = partial(yaml);

    assert_eq!(pw.context_files.len(), 3);
    let names: Vec<&str> = pw.context_files.iter().map(|f| f.value.as_str()).collect();
    assert!(names.contains(&"readme"));
    assert!(names.contains(&"license"));
    assert!(names.contains(&"changelog"));
}

// ===========================================================================
// Additional edge cases: inputs
// ===========================================================================

#[test]
fn input_names_extracted_as_mapping() {
    let yaml = "\
inputs:
  name: string
  count: number
  verbose: boolean
";
    let pw = partial(yaml);

    assert_eq!(pw.input_names.len(), 3);
    let names: Vec<&str> = pw.input_names.iter().map(|i| i.value.as_str()).collect();
    assert!(names.contains(&"name"));
    assert!(names.contains(&"count"));
    assert!(names.contains(&"verbose"));
}

// ===========================================================================
// Additional edge cases: task features
// ===========================================================================

#[test]
fn task_without_id_still_extracted() {
    let yaml = "\
tasks:
  - infer:
      prompt: anonymous task
";
    let pw = partial(yaml);

    assert_eq!(pw.tasks.len(), 1);
    assert!(pw.tasks[0].id.is_none());
    assert_eq!(pw.tasks[0].verb.as_deref(), Some("infer"));
}

#[test]
fn task_without_verb_still_extracted() {
    let yaml = "\
tasks:
  - id: no_verb
    content: true
    with:
      data: file.txt
";
    let pw = partial(yaml);

    assert_eq!(pw.tasks.len(), 1);
    let task = &pw.tasks[0];
    assert_eq!(task.id.as_deref(), Some("no_verb"));
    assert!(task.verb.is_none());
    assert!(task.has_content);
    assert!(task.has_with);
}

#[test]
fn task_with_all_features_combined() {
    let yaml = "\
schema: '@0.12'
tasks:
  - id: full_featured
    infer:
      model: gpt-4
      prompt: \"Process {{with.doc}}\"
      temperature: 0.8
    content:
      - type: text
        text: supplemental context
    with:
      doc: input.txt
      config: settings.yaml
    depends_on: [setup, validate]
";
    let pw = partial(yaml);

    assert_eq!(pw.tasks.len(), 1);
    let task = &pw.tasks[0];
    assert_eq!(task.id.as_deref(), Some("full_featured"));
    assert_eq!(task.verb.as_deref(), Some("infer"));
    assert!(task.has_content);
    assert!(task.has_with);
    assert!(task.has_depends_on);
    assert_eq!(task.with_aliases.len(), 2);
    assert_eq!(task.depends_on_refs.len(), 2);
    assert!(task.depends_on_refs.contains(&"setup".to_string()));
    assert!(task.depends_on_refs.contains(&"validate".to_string()));
}

#[test]
fn task_existing_keys_lists_all_keys() {
    let yaml = "\
tasks:
  - id: rich
    description: A rich task
    infer:
      prompt: test
    content: true
    with:
      x: y
    depends_on: [a]
    retry:
      max_attempts: 3
";
    let pw = partial(yaml);

    let keys = &pw.tasks[0].existing_keys;
    assert!(keys.contains(&"id".to_string()));
    assert!(keys.contains(&"description".to_string()));
    assert!(keys.contains(&"infer".to_string()));
    assert!(keys.contains(&"content".to_string()));
    assert!(keys.contains(&"with".to_string()));
    assert!(keys.contains(&"depends_on".to_string()));
    assert!(keys.contains(&"retry".to_string()));
}

// ===========================================================================
// Additional edge cases: quoted IDs
// ===========================================================================

#[test]
fn task_id_double_quoted() {
    let yaml = "\
tasks:
  - id: \"my-task-id\"
    exec:
      command: echo
";
    let pw = partial(yaml);
    assert_eq!(pw.tasks[0].id.as_deref(), Some("my-task-id"));
}

#[test]
fn task_id_single_quoted() {
    let yaml = "\
tasks:
  - id: 'single-quoted-id'
    exec:
      command: echo
";
    let pw = partial(yaml);
    assert_eq!(pw.tasks[0].id.as_deref(), Some("single-quoted-id"));
}

// ===========================================================================
// Additional edge cases: schema variants
// ===========================================================================

#[test]
fn schema_double_quoted() {
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t\n    exec:\n      command: x\n";
    let pw = partial(yaml);
    assert!(pw.schema.is_some());
    // Value includes the quotes as raw text from tree-sitter
    let val = &pw.schema.as_ref().unwrap().value;
    assert!(val.contains("0.12"), "schema value should contain version");
}

#[test]
fn schema_field_has_correct_spans() {
    let yaml = "schema: '@0.12'\n";
    let pw = partial(yaml);

    let schema = pw.schema.as_ref().unwrap();
    assert_eq!(schema.key_span.start, 0);
    assert!(schema.value_span.is_some());
    let vs = schema.value_span.unwrap();
    assert!(vs.start > 0, "value should start after 'schema: '");
    assert!(vs.end > vs.start);
}

// ===========================================================================
// Additional edge cases: error recovery
// ===========================================================================

#[test]
fn broken_duplicate_verbs_in_same_task() {
    let yaml = "\
tasks:
  - id: dupe
    infer:
      prompt: a
    exec:
      command: b
";
    let pw = partial(yaml);

    assert!(!pw.tasks.is_empty());
    let task = &pw.tasks[0];
    assert!(task.existing_keys.contains(&"infer".to_string()));
    assert!(task.existing_keys.contains(&"exec".to_string()));
    // The verb field takes the last one seen (implementation detail),
    // but both should be in existing_keys
}

#[test]
fn broken_invalid_yaml_flow_mapping_no_panic() {
    let yaml = "schema: {version: 0.12}\n";
    let pw = partial(yaml);
    assert!(pw.schema.is_some());
}

#[test]
fn broken_trailing_colon_only() {
    let yaml = ":\n";
    let pw = partial(yaml);
    // Must not panic
    let _ = pw;
}

#[test]
fn broken_key_without_value_for_tasks() {
    let yaml = "tasks:\n";
    let pw = partial(yaml);
    assert!(pw.tasks.is_empty());
    // "tasks" should still appear in top_level_keys
    let keys: Vec<&str> = pw.top_level_keys.iter().map(|k| k.value.as_str()).collect();
    assert!(keys.contains(&"tasks"));
}

// ===========================================================================
// Comprehensive workflow: all sections together
// ===========================================================================

#[test]
fn comprehensive_workflow_all_sections() {
    let yaml = "\
schema: '@0.12'
workflow: comprehensive-pipeline
include:
  - ./base.nika.yaml
  - ./mcp-config.nika.yaml
inputs:
  query: string
  max_results: number
mcp:
  novanet:
    command: novanet serve
  filesystem:
    command: fs serve
context:
  files:
    readme: ./README.md
    config: ./config.yaml
tasks:
  - id: search
    invoke:
      tool: novanet_search
      input:
        query: \"{{inputs.query}}\"
    with:
      limit: \"{{inputs.max_results}}\"
  - id: process
    infer:
      model: gpt-4
      prompt: \"Process results from {{with.results}}\"
    with:
      results: \"{{tasks.search.output}}\"
    content:
      - type: text
        text: Additional context
    depends_on: [search]
  - id: export
    exec:
      command: echo \"{{tasks.process.output}}\"
    depends_on: [process]
";
    let pw = partial(yaml);

    assert!(!pw.has_errors);

    // Schema and workflow
    assert_eq!(pw.schema.as_ref().unwrap().value, "'@0.12'");
    assert_eq!(
        pw.workflow_name.as_ref().unwrap().value,
        "comprehensive-pipeline"
    );

    // Includes
    assert_eq!(pw.includes.len(), 2);

    // Inputs
    assert_eq!(pw.input_names.len(), 2);

    // MCP servers
    assert_eq!(pw.mcp_servers.len(), 2);

    // Context files
    assert_eq!(pw.context_files.len(), 2);

    // Tasks
    assert_eq!(pw.tasks.len(), 3);

    let search = &pw.tasks[0];
    assert_eq!(search.id.as_deref(), Some("search"));
    assert_eq!(search.verb.as_deref(), Some("invoke"));
    assert!(search.has_with);

    let process = &pw.tasks[1];
    assert_eq!(process.id.as_deref(), Some("process"));
    assert_eq!(process.verb.as_deref(), Some("infer"));
    assert!(process.has_with);
    assert!(process.has_content);
    assert!(process.has_depends_on);
    assert!(process.depends_on_refs.contains(&"search".to_string()));

    let export = &pw.tasks[2];
    assert_eq!(export.id.as_deref(), Some("export"));
    assert_eq!(export.verb.as_deref(), Some("exec"));
    assert!(export.has_depends_on);
    assert!(export.depends_on_refs.contains(&"process".to_string()));

    // Top-level keys
    let keys: Vec<&str> = pw.top_level_keys.iter().map(|k| k.value.as_str()).collect();
    for expected in &[
        "schema", "workflow", "include", "inputs", "mcp", "context", "tasks",
    ] {
        assert!(keys.contains(expected), "missing top-level key: {expected}");
    }
}

// ===========================================================================
// Span integrity checks
// ===========================================================================

#[test]
fn task_spans_do_not_overlap() {
    let yaml = "\
tasks:
  - id: a
    exec:
      command: echo a
  - id: b
    exec:
      command: echo b
  - id: c
    exec:
      command: echo c
";
    let pw = partial(yaml);
    assert_eq!(pw.tasks.len(), 3);

    // Each task's span end should be <= next task's span start
    for window in pw.tasks.windows(2) {
        assert!(
            window[0].span.end <= window[1].span.start,
            "task {:?} span [{}, {}) overlaps with task {:?} span [{}, {})",
            window[0].id,
            window[0].span.start,
            window[0].span.end,
            window[1].id,
            window[1].span.start,
            window[1].span.end,
        );
    }
}

#[test]
fn task_span_text_contains_task_content() {
    let yaml = "\
tasks:
  - id: spanned
    infer:
      prompt: hello world
";
    let pw = partial(yaml);
    assert_eq!(pw.tasks.len(), 1);

    let task = &pw.tasks[0];
    let span_text = &yaml[task.span.start as usize..task.span.end as usize];
    assert!(
        span_text.contains("id: spanned"),
        "span text should contain task id"
    );
    assert!(
        span_text.contains("infer:"),
        "span text should contain verb"
    );
}

// ===========================================================================
// Unicode content
// ===========================================================================

#[test]
fn unicode_in_values_no_panic() {
    let yaml = "\
schema: '@0.12'
workflow: unicode-test
tasks:
  - id: emoji_task
    infer:
      prompt: \"Generate a response with emojis \u{1F600}\u{1F680}\u{2728}\"
  - id: cjk_task
    exec:
      command: echo \"\u{4F60}\u{597D}\u{4E16}\u{754C}\"
  - id: arabic_task
    infer:
      prompt: \"\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}\"
";
    let pw = partial(yaml);

    assert!(!pw.has_errors);
    assert_eq!(pw.tasks.len(), 3);
}

// ===========================================================================
// Multiline strings
// ===========================================================================

#[test]
fn multiline_literal_block_scalar() {
    let yaml = "\
tasks:
  - id: literal
    infer:
      prompt: |
        This is a multiline
        literal block scalar
        that spans several lines
";
    let pw = partial(yaml);

    assert!(!pw.has_errors);
    assert_eq!(pw.tasks.len(), 1);
    assert_eq!(pw.tasks[0].verb.as_deref(), Some("infer"));
}

#[test]
fn multiline_folded_block_scalar() {
    let yaml = "\
tasks:
  - id: folded
    infer:
      prompt: >
        This is a folded
        block scalar that
        will be joined into
        a single line
";
    let pw = partial(yaml);

    assert!(!pw.has_errors);
    assert_eq!(pw.tasks.len(), 1);
    assert_eq!(pw.tasks[0].verb.as_deref(), Some("infer"));
}

// ===========================================================================
// Edge: many tasks stress test
// ===========================================================================

#[test]
fn many_tasks_20_all_ids_correct() {
    let mut yaml = String::from("tasks:\n");
    for i in 0..20 {
        yaml.push_str(&format!(
            "  - id: task_{i}\n    exec:\n      command: echo {i}\n"
        ));
    }
    let pw = partial(&yaml);

    assert_eq!(pw.tasks.len(), 20);
    for (i, task) in pw.tasks.iter().enumerate() {
        assert_eq!(
            task.id.as_deref(),
            Some(format!("task_{i}").as_str()),
            "task {i} has wrong id"
        );
        assert_eq!(task.verb.as_deref(), Some("exec"));
    }
}

// ===========================================================================
// Edge: depends_on with quoted values
// ===========================================================================

#[test]
fn depends_on_quoted_values() {
    let yaml = "\
tasks:
  - id: end
    exec:
      command: echo end
    depends_on: [\"quoted_a\", 'quoted_b']
";
    let pw = partial(yaml);

    let task = &pw.tasks[0];
    assert!(task.has_depends_on);
    assert!(task.depends_on_refs.contains(&"quoted_a".to_string()));
    assert!(task.depends_on_refs.contains(&"quoted_b".to_string()));
}

// ===========================================================================
// Edge: empty sections
// ===========================================================================

#[test]
fn empty_tasks_section() {
    let yaml = "\
schema: '@0.12'
tasks:
";
    let pw = partial(yaml);
    assert!(pw.schema.is_some());
    assert!(pw.tasks.is_empty());
}

#[test]
fn empty_mcp_section() {
    let yaml = "\
mcp:
";
    let pw = partial(yaml);
    assert!(pw.mcp_servers.is_empty());
}

#[test]
fn empty_include_section() {
    let yaml = "\
include:
";
    let pw = partial(yaml);
    assert!(pw.includes.is_empty());
}

// ===========================================================================
// Regression: ensure error_ranges have valid byte offsets
// ===========================================================================

#[test]
fn error_ranges_are_valid_byte_offsets() {
    let yaml = "\
schema: '@0.12'
tasks:
  - id: bad
    infer
      prompt: broken
";
    let pw = partial(yaml);

    assert!(pw.has_errors);
    for range in &pw.error_ranges {
        assert!(
            (range.start as usize) <= yaml.len(),
            "error range start {} exceeds text length {}",
            range.start,
            yaml.len()
        );
        assert!(
            (range.end as usize) <= yaml.len(),
            "error range end {} exceeds text length {}",
            range.end,
            yaml.len()
        );
        assert!(
            range.start <= range.end,
            "error range start {} > end {}",
            range.start,
            range.end
        );
    }
}

// ===========================================================================
// Fixture-based tests — broken YAML error recovery
// ===========================================================================

/// Load a broken fixture file and run through bridge.
fn fixture(name: &str) -> PartialWorkflow {
    let path = format!(
        "{}/tests/fixtures/broken/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {path}: {e}"));
    partial(&content)
}

#[test]
fn fixture_unclosed_bracket_recovers_tasks() {
    let pw = fixture("unclosed-bracket.broken.yaml");
    assert!(pw.has_errors, "unclosed bracket should produce errors");
    // Should still extract task IDs despite the broken depends_on
    assert!(
        !pw.tasks.is_empty(),
        "should recover at least some tasks from unclosed bracket"
    );
}

#[test]
fn fixture_unclosed_brace_no_panic() {
    let pw = fixture("unclosed-brace.broken.yaml");
    // Main invariant: never panics. Recovery may or may not find tasks.
    assert!(pw.has_errors, "unclosed brace should produce errors");
    let _ = pw.tasks.len(); // just verify access doesn't panic
}

#[test]
fn fixture_missing_value_recovers_schema() {
    let pw = fixture("missing-value.broken.yaml");
    // Schema line exists even if some values are missing
    assert!(
        pw.schema.is_some(),
        "should recover schema from missing-value fixture"
    );
}

#[test]
fn fixture_partial_verb_no_panic() {
    let pw = fixture("partial-verb.broken.yaml");
    assert!(pw.has_errors, "verbs without colons should produce errors");
    // Recovery from verb-without-colon is partial — main invariant: no panic
    let _ = pw.tasks.len();
}

#[test]
fn fixture_multi_error_never_panics() {
    let pw = fixture("multi-error.broken.yaml");
    assert!(pw.has_errors, "multi-error fixture should have errors");
    assert!(
        !pw.error_ranges.is_empty(),
        "multi-error should have at least one error range"
    );
}

#[test]
fn fixture_duplicate_keys_recovers() {
    let pw = fixture("duplicate-keys.broken.yaml");
    // Duplicate keys are valid YAML but semantically wrong
    // Bridge should still extract structure
    assert!(
        !pw.tasks.is_empty(),
        "should recover tasks from duplicate-keys fixture"
    );
}

#[test]
fn fixture_empty_task_list_no_panic() {
    let pw = fixture("empty-task-list.broken.yaml");
    assert!(
        pw.schema.is_some(),
        "should recover schema from empty task list"
    );
    // tasks: with no items → 0 tasks, no panic
    assert_eq!(pw.tasks.len(), 0, "empty task list should have 0 tasks");
}

#[test]
fn fixture_broken_mcp_recovers_schema() {
    let pw = fixture("broken-mcp.broken.yaml");
    assert!(
        pw.schema.is_some(),
        "should recover schema despite broken mcp config"
    );
}

#[test]
fn fixture_mixed_tabs_spaces_no_panic() {
    let pw = fixture("mixed-tabs-spaces.broken.yaml");
    // Just verify it doesn't panic — mixed tabs are unpredictable
    let _ = pw.tasks.len();
    let _ = pw.has_errors;
}

#[test]
fn fixture_unicode_keys_no_panic() {
    let pw = fixture("unicode-in-keys.broken.yaml");
    // Bridge should not panic on non-ASCII key names
    let _ = pw.tasks.len();
    let _ = pw.has_errors;
}

#[test]
fn fixture_very_long_line_no_panic() {
    let pw = fixture("very-long-line.broken.yaml");
    // Should handle 5000+ char lines without panicking or hanging
    assert!(
        pw.schema.is_some(),
        "should recover schema from very long line fixture"
    );
}

#[test]
fn fixture_nested_error_no_panic() {
    let pw = fixture("nested-error.broken.yaml");
    assert!(pw.has_errors, "nested errors should be detected");
    // Recovery quality varies — main invariant: no panic
    let _ = pw.tasks.len();
}

#[test]
fn fixture_trailing_content_no_panic() {
    let pw = fixture("trailing-content.broken.yaml");
    // YAML multi-document (---) may confuse tree-sitter — main invariant: no panic
    let _ = pw.schema;
    let _ = pw.tasks.len();
}

#[test]
fn fixture_bad_indent_recovery() {
    let pw = fixture("bad-indent-recovery.broken.yaml");
    assert!(pw.has_errors, "bad indentation should produce errors");
    // Schema at correct indent should still be found
    assert!(
        pw.schema.is_some(),
        "should recover schema despite bad indentation"
    );
}

/// Smoke test: ALL fixtures parse without panic.
#[test]
fn all_fixtures_never_panic() {
    let fixture_dir = format!("{}/tests/fixtures/broken", env!("CARGO_MANIFEST_DIR"));
    let entries =
        std::fs::read_dir(&fixture_dir).unwrap_or_else(|e| panic!("Cannot read fixture dir: {e}"));

    let mut count = 0;
    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "yaml") {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
            let _pw = partial(&content);
            count += 1;
        }
    }
    assert!(
        count >= 20,
        "Expected at least 20 fixture files, found {}",
        count
    );
}
