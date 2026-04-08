// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tests for the analyzer module.

use super::*;
use crate::ast::raw::{
    RawContextConfig, RawIncludeSpec, RawMcpConfig, RawMcpServer, RawTask, RawWorkflow,
};
use crate::source::{FileId, Spanned};
use indexmap::IndexMap;

fn make_span(start: u32, end: u32) -> Span {
    Span::new(FileId(0), start, end)
}

fn make_raw_workflow(schema: &str, tasks: Vec<RawTask>) -> RawWorkflow {
    RawWorkflow {
        schema: Spanned::new(schema.to_string(), make_span(0, 20)),
        tasks: Spanned::new(
            tasks
                .into_iter()
                .map(|t| Spanned::new(t, make_span(0, 50)))
                .collect(),
            make_span(0, 100),
        ),
        ..Default::default()
    }
}

fn make_raw_task(id: &str) -> RawTask {
    use crate::ast::raw::{RawExecAction, RawTaskAction};
    RawTask {
        id: Spanned::new(id.to_string(), make_span(0, id.len() as u32)),
        action: Some(RawTaskAction::Exec(Spanned::new(
            RawExecAction {
                command: Spanned::new("echo test".to_string(), make_span(0, 9)),
                shell: None,
                cwd: None,
                env: None,
                timeout_ms: None,
                max_stdout: None,
            },
            make_span(0, 20),
        ))),
        ..Default::default()
    }
}

/// Helper: add a `with:` binding to a raw task.
fn add_with_ref(task: &mut RawTask, alias: &str, expr: &str) {
    let with_refs = task
        .with_refs
        .get_or_insert_with(|| Spanned::new(IndexMap::new(), make_span(0, 50)));
    with_refs.value.insert(
        Spanned::new(alias.to_string(), make_span(0, alias.len() as u32)),
        Spanned::new(expr.to_string(), make_span(0, expr.len() as u32)),
    );
}

/// Helper: add `depends_on:` entries to a raw task.
fn add_depends_on(task: &mut RawTask, deps: &[&str]) {
    let spanned_deps: Vec<Spanned<String>> = deps
        .iter()
        .map(|d| Spanned::new(d.to_string(), make_span(0, d.len() as u32)))
        .collect();
    task.depends_on = Some(Spanned::new(spanned_deps, make_span(0, 50)));
}

// ====================================================================
// Basic workflow analysis
// ====================================================================

#[test]
fn test_analyze_valid_workflow() {
    let raw = make_raw_workflow(
        "nika/workflow@0.12",
        vec![make_raw_task("task1"), make_raw_task("task2")],
    );

    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    assert_eq!(workflow.task_count(), 2);
    assert!(workflow.has_task("task1"));
    assert!(workflow.has_task("task2"));
}

#[test]
fn test_analyze_invalid_schema() {
    let raw = make_raw_workflow("invalid", vec![]);
    let result = analyze(raw);
    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::InvalidSchema);
}

#[test]
fn test_analyze_schema_suggestion() {
    // Valid schema with at least one task
    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("step1")]);
    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);
}

#[test]
fn test_analyze_duplicate_task() {
    let raw = make_raw_workflow(
        "nika/workflow@0.12",
        vec![make_raw_task("task1"), make_raw_task("task1")],
    );

    let result = analyze(raw);
    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::DuplicateTask);
}

// ====================================================================
// with: binding analysis
// ====================================================================

#[test]
fn test_analyze_with_binding_simple() {
    let mut task2 = make_raw_task("task2");
    add_with_ref(&mut task2, "data", "$task1");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    let t2 = workflow.get_task_by_name("task2").unwrap();
    assert_eq!(t2.with_spec.len(), 1);
    assert!(t2.with_spec.contains_key("data"));
    assert_eq!(t2.implicit_deps.len(), 1);
}

#[test]
fn test_analyze_with_binding_unknown_task() {
    let mut task1 = make_raw_task("task1");
    add_with_ref(&mut task1, "data", "$unknown_task");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::UnknownTask);
}

#[test]
fn test_analyze_with_binding_invalid_expr() {
    let mut task1 = make_raw_task("task1");
    // Empty expression should fail parse_with_entry
    add_with_ref(&mut task1, "data", "");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::InvalidBinding);
}

#[test]
fn test_analyze_with_binding_env_source() {
    // $env.API_KEY should NOT create an implicit dependency
    let mut task1 = make_raw_task("task1");
    add_with_ref(&mut task1, "key", "$env.API_KEY");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    let t1 = workflow.get_task_by_name("task1").unwrap();
    assert_eq!(t1.with_spec.len(), 1);
    assert!(t1.implicit_deps.is_empty()); // Env source, no task dep
}

#[test]
fn test_analyze_with_binding_deduplicates_implicit_deps() {
    let mut task2 = make_raw_task("task2");
    add_with_ref(&mut task2, "a", "$task1.field_a");
    add_with_ref(&mut task2, "b", "$task1.field_b");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    let t2 = workflow.get_task_by_name("task2").unwrap();
    // Both bindings reference task1, but implicit_deps should deduplicate
    assert_eq!(t2.implicit_deps.len(), 1);
}

#[test]
fn test_analyze_with_binding_with_transforms() {
    let mut task2 = make_raw_task("task2");
    add_with_ref(&mut task2, "result", "$task1.data | upper | trim");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    let t2 = workflow.get_task_by_name("task2").unwrap();
    let entry = t2.with_spec.get("result").unwrap();
    assert!(entry.transform.is_some());
    assert_eq!(entry.task_id(), Some("task1"));
}

#[test]
fn test_analyze_with_binding_with_default() {
    let mut task2 = make_raw_task("task2");
    add_with_ref(&mut task2, "val", "$task1.count ?? 0");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    let t2 = workflow.get_task_by_name("task2").unwrap();
    let entry = t2.with_spec.get("val").unwrap();
    assert!(entry.default.is_some());
}

// ====================================================================
// depends_on: analysis
// ====================================================================

#[test]
fn test_analyze_depends_on_valid() {
    let mut task2 = make_raw_task("task2");
    add_depends_on(&mut task2, &["task1"]);

    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    let t2 = workflow.get_task_by_name("task2").unwrap();
    assert_eq!(t2.depends_on.len(), 1);
}

#[test]
fn test_analyze_depends_on_unknown_task() {
    let mut task1 = make_raw_task("task1");
    add_depends_on(&mut task1, &["nonexistent"]);

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::UnknownTask);
}

#[test]
fn test_analyze_depends_on_multiple() {
    let mut task3 = make_raw_task("task3");
    add_depends_on(&mut task3, &["task1", "task2"]);

    let raw = make_raw_workflow(
        "nika/workflow@0.12",
        vec![make_raw_task("task1"), make_raw_task("task2"), task3],
    );
    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    let t3 = workflow.get_task_by_name("task3").unwrap();
    assert_eq!(t3.depends_on.len(), 2);
}

// ====================================================================
// Cycle detection
// ====================================================================

#[test]
fn test_analyze_cyclic_dependency_depends_on() {
    let mut task1 = make_raw_task("task1");
    let mut task2 = make_raw_task("task2");

    add_depends_on(&mut task1, &["task2"]);
    add_depends_on(&mut task2, &["task1"]);

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
}

#[test]
fn test_analyze_cyclic_dependency_via_with() {
    let mut task1 = make_raw_task("task1");
    let mut task2 = make_raw_task("task2");

    add_with_ref(&mut task1, "data", "$task2");
    add_with_ref(&mut task2, "data", "$task1");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
}

#[test]
fn test_analyze_cyclic_dependency_mixed() {
    // task1 depends_on task2 explicitly, task2 references task1 via with:
    let mut task1 = make_raw_task("task1");
    let mut task2 = make_raw_task("task2");

    add_depends_on(&mut task1, &["task2"]);
    add_with_ref(&mut task2, "data", "$task1");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
}

#[test]
fn test_analyze_cyclic_dependency_three_tasks_via_with() {
    // A → B → C → A via with: bindings (transitive implicit cycle)
    let mut a = make_raw_task("a");
    add_with_ref(&mut a, "x", "$c.out");

    let mut b = make_raw_task("b");
    add_with_ref(&mut b, "x", "$a.out");

    let mut c = make_raw_task("c");
    add_with_ref(&mut c, "x", "$b.out");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![a, b, c]);
    let result = analyze(raw);

    assert!(result.is_err(), "3-task implicit cycle should be detected");
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
}

#[test]
fn test_analyze_complex_jsonpath_extracts_dep() {
    // Deep JSONPath with transforms and default: $task1.data.items | sort | first(3) ?? []
    let mut task2 = make_raw_task("task2");
    add_with_ref(
        &mut task2,
        "items",
        "$task1.data.items | sort | first(3) ?? []",
    );

    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let wf = result.value.unwrap();
    let t2 = wf.get_task_by_name("task2").unwrap();
    assert_eq!(
        t2.implicit_deps.len(),
        1,
        "should extract dep from complex JSONPath expression"
    );
}

// ====================================================================
// include: analysis
// ====================================================================

#[test]
fn test_analyze_includes() {
    use crate::ast::raw::RawIncludeSpec;

    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);
    raw.include = Some(Spanned::new(
        vec![
            Spanned::new(
                RawIncludeSpec {
                    path: Spanned::new(
                        "./partials/setup.nika.yaml".to_string(),
                        make_span(0, 25),
                    ),
                    prefix: Some(Spanned::new("setup_".to_string(), make_span(30, 36))),
                    span: make_span(0, 40),
                },
                make_span(0, 40),
            ),
            Spanned::new(
                RawIncludeSpec {
                    path: Spanned::new("./tools.nika.yaml".to_string(), make_span(50, 67)),
                    prefix: None,
                    span: make_span(50, 70),
                },
                make_span(50, 70),
            ),
        ],
        make_span(0, 80),
    ));

    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    assert_eq!(workflow.include.len(), 2);
    assert_eq!(workflow.include[0].path, "./partials/setup.nika.yaml");
    assert_eq!(workflow.include[0].prefix.as_deref(), Some("setup_"));
    assert_eq!(workflow.include[1].path, "./tools.nika.yaml");
    assert!(workflow.include[1].prefix.is_none());
}

// ====================================================================
// Feature gating tests
// ====================================================================

#[test]
fn test_feature_gate_for_each_v01_fails() {
    use crate::ast::raw::RawForEach;

    let mut task = make_raw_task("task1");
    task.for_each = Some(Spanned::new(
        RawForEach {
            items: Spanned::new("[\"a\", \"b\"]".to_string(), make_span(0, 10)),
            as_var: None,
            concurrency: None,
            fail_fast: None,
        },
        make_span(0, 50),
    ));

    // Using v0.1 which doesn't support for_each
    let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
    assert!(result.errors[0].message.contains("for_each"));
}

#[test]
fn test_feature_gate_for_each_v03_succeeds() {
    use crate::ast::raw::RawForEach;

    let mut task = make_raw_task("task1");
    task.for_each = Some(Spanned::new(
        RawForEach {
            items: Spanned::new("[\"a\", \"b\"]".to_string(), make_span(0, 10)),
            as_var: None,
            concurrency: None,
            fail_fast: None,
        },
        make_span(0, 50),
    ));

    // Using v0.3 which supports for_each
    let raw = make_raw_workflow("nika/workflow@0.3", vec![task]);
    let result = analyze(raw);

    // Should not have UnsupportedFeature errors
    assert!(!result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
}

#[test]
fn test_feature_gate_retry_v02_fails() {
    use crate::ast::raw::RawRetryConfig;

    let mut task = make_raw_task("task1");
    task.retry = Some(Spanned::new(
        RawRetryConfig {
            max_attempts: Some(Spanned::new(Templatable::Value(3), make_span(0, 1))),
            delay_ms: None,
            backoff: None,
        },
        make_span(0, 30),
    ));

    // Using v0.2 which doesn't support retry
    let raw = make_raw_workflow("nika/workflow@0.2", vec![task]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
    assert!(result.errors[0].message.contains("retry"));
}

#[test]
fn test_feature_gate_invoke_v01_fails() {
    use crate::ast::raw::RawInvokeAction;

    let mut task = make_raw_task("task1");
    task.action = Some(RawTaskAction::Invoke(Spanned::new(
        RawInvokeAction {
            tool: Some(Spanned::new("novanet:search".to_string(), make_span(0, 14))),
            resource: None,
            mcp: None,
            params: None,
            timeout_ms: None,
        },
        make_span(0, 50),
    )));

    // Using v0.1 which doesn't support invoke
    let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
    assert!(result.errors[0].message.contains("invoke"));
}

#[test]
fn test_feature_gate_agent_v01_fails() {
    use crate::ast::raw::RawAgentAction;

    let mut task = make_raw_task("task1");
    task.action = Some(RawTaskAction::Agent(Box::new(Spanned::new(
        RawAgentAction {
            prompt: Spanned::new("Do something".to_string(), make_span(0, 12)),
            tools: None,
            max_turns: None,
            max_tokens: None,
            from: None,
            skills: None,
            provider: None,
            model: None,
            mcp: None,
            system: None,
            temperature: None,
            token_budget: None,
            extended_thinking: None,
            thinking_budget: None,
            depth_limit: None,
            tool_choice: None,
            stop_sequences: None,
            scope: None,
            guardrails: Vec::new(),
            completion: None,
            limits: None,
        },
        make_span(0, 50),
    ))));

    // Using v0.1 which doesn't support agent
    let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
    assert!(result.errors[0].message.contains("agent"));
}

#[test]
fn test_feature_gate_with_v11_fails() {
    let mut task = make_raw_task("task1");
    add_with_ref(&mut task, "data", "$other");

    // Doesn't support with:
    let raw = make_raw_workflow("nika/workflow@0.11", vec![task]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
}

#[test]
fn test_feature_gate_depends_on_v11_fails() {
    let mut task = make_raw_task("task1");
    add_depends_on(&mut task, &["other"]);

    // Doesn't support depends_on:
    let raw = make_raw_workflow("nika/workflow@0.11", vec![task]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
}

#[test]
fn test_feature_gate_include_v11_fails() {
    use crate::ast::raw::RawIncludeSpec;

    let mut raw = make_raw_workflow("nika/workflow@0.11", vec![make_raw_task("task1")]);
    raw.include = Some(Spanned::new(
        vec![Spanned::new(
            RawIncludeSpec {
                path: Spanned::new("./setup.nika.yaml".to_string(), make_span(0, 17)),
                prefix: None,
                span: make_span(0, 20),
            },
            make_span(0, 20),
        )],
        make_span(0, 30),
    ));

    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
}

#[test]
fn test_feature_gate_multiple_errors() {
    use crate::ast::raw::{RawAgentAction, RawForEach};

    let mut task = make_raw_task("task1");

    // Add both for_each and agent action
    task.for_each = Some(Spanned::new(
        RawForEach {
            items: Spanned::new("[\"a\"]".to_string(), make_span(0, 5)),
            as_var: None,
            concurrency: None,
            fail_fast: None,
        },
        make_span(0, 30),
    ));
    task.action = Some(RawTaskAction::Agent(Box::new(Spanned::new(
        RawAgentAction {
            prompt: Spanned::new("Goal".to_string(), make_span(0, 4)),
            tools: None,
            max_turns: None,
            max_tokens: None,
            from: None,
            skills: None,
            provider: None,
            model: None,
            mcp: None,
            system: None,
            temperature: None,
            token_budget: None,
            extended_thinking: None,
            thinking_budget: None,
            depth_limit: None,
            tool_choice: None,
            stop_sequences: None,
            scope: None,
            guardrails: Vec::new(),
            completion: None,
            limits: None,
        },
        make_span(0, 50),
    ))));

    // Using v0.1 which doesn't support either
    let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
    let result = analyze(raw);

    // Should have multiple UnsupportedFeature errors
    let feature_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature)
        .collect();
    assert_eq!(feature_errors.len(), 2);
}

#[test]
fn test_feature_gate_error_message_format() {
    use crate::ast::raw::RawForEach;

    let mut task = make_raw_task("task1");
    task.for_each = Some(Spanned::new(
        RawForEach {
            items: Spanned::new("[\"x\"]".to_string(), make_span(0, 5)),
            as_var: None,
            concurrency: None,
            fail_fast: None,
        },
        make_span(0, 30),
    ));

    let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
    let result = analyze(raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    let err = &result.errors[0];
    assert!(err.message.contains("requires schema version"));
    assert!(err.message.contains("nika/workflow@0.3"));
    assert!(err.message.contains("nika/workflow@0.1"));
    assert!(err.suggestion.as_ref().unwrap().contains("upgrade"));
}

// ====================================================================
// Metadata extraction
// ====================================================================

#[test]
fn test_analyze_metadata() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);
    raw.workflow = Some(Spanned::new("my-workflow".to_string(), make_span(0, 11)));
    raw.description = Some(Spanned::new(
        "A test workflow".to_string(),
        make_span(0, 15),
    ));
    raw.provider = Some(Spanned::new("claude".to_string(), make_span(0, 6)));
    raw.model = Some(Spanned::new(
        "claude-sonnet-4-6".to_string(),
        make_span(0, 15),
    ));

    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    assert_eq!(workflow.name.as_deref(), Some("my-workflow"));
    assert_eq!(workflow.description.as_deref(), Some("A test workflow"));
    assert_eq!(workflow.provider, Some(crate::ProviderName::Anthropic));
    assert_eq!(workflow.model.as_deref(), Some("claude-sonnet-4-6"));
}

// ====================================================================
// Inputs analysis
// ====================================================================

#[test]
fn test_analyze_inputs() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);

    let mut inputs = IndexMap::new();
    inputs.insert(
        Spanned::new("topic".to_string(), make_span(0, 5)),
        Spanned::new(serde_json::Value::String("AI".to_string()), make_span(0, 4)),
    );
    inputs.insert(
        Spanned::new("count".to_string(), make_span(0, 5)),
        Spanned::new(serde_json::json!(3), make_span(0, 1)),
    );
    raw.inputs = Some(Spanned::new(inputs, make_span(0, 50)));

    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    assert_eq!(workflow.inputs.len(), 2);
    assert_eq!(
        workflow.inputs.get("topic"),
        Some(&serde_json::Value::String("AI".to_string()))
    );
}

// ====================================================================
// validate() tests
// ====================================================================

#[test]
fn test_validate_valid_workflow() {
    let raw = make_raw_workflow(
        "nika/workflow@0.12",
        vec![make_raw_task("task1"), make_raw_task("task2")],
    );

    let result = validate(&raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);
}

#[test]
fn test_validate_invalid_schema() {
    let raw = make_raw_workflow("invalid", vec![]);
    let result = validate(&raw);
    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::InvalidSchema);
}

#[test]
fn test_validate_duplicate_task() {
    let raw = make_raw_workflow(
        "nika/workflow@0.12",
        vec![make_raw_task("task1"), make_raw_task("task1")],
    );

    let result = validate(&raw);
    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::DuplicateTask);
}

#[test]
fn test_validate_unknown_task_in_with() {
    let mut task1 = make_raw_task("task1");
    add_with_ref(&mut task1, "data", "$unknown_task");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
    let result = validate(&raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::UnknownTask);
}

#[test]
fn test_validate_unknown_task_in_depends_on() {
    let mut task1 = make_raw_task("task1");
    add_depends_on(&mut task1, &["nonexistent"]);

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
    let result = validate(&raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::UnknownTask);
}

#[test]
fn test_validate_invalid_binding() {
    let mut task1 = make_raw_task("task1");
    add_with_ref(&mut task1, "data", "");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);
    let result = validate(&raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::InvalidBinding);
}

#[test]
fn test_validate_cyclic_dependency_depends_on() {
    let mut task1 = make_raw_task("task1");
    let mut task2 = make_raw_task("task2");

    add_depends_on(&mut task1, &["task2"]);
    add_depends_on(&mut task2, &["task1"]);

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
    let result = validate(&raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
}

#[test]
fn test_validate_cyclic_dependency_via_with() {
    let mut task1 = make_raw_task("task1");
    let mut task2 = make_raw_task("task2");

    add_with_ref(&mut task1, "data", "$task2");
    add_with_ref(&mut task2, "data", "$task1");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
    let result = validate(&raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
}

#[test]
fn test_validate_feature_gate() {
    use crate::ast::raw::RawForEach;

    let mut task = make_raw_task("task1");
    task.for_each = Some(Spanned::new(
        RawForEach {
            items: Spanned::new("[\"a\"]".to_string(), make_span(0, 5)),
            as_var: None,
            concurrency: None,
            fail_fast: None,
        },
        make_span(0, 30),
    ));

    let raw = make_raw_workflow("nika/workflow@0.1", vec![task]);
    let result = validate(&raw);

    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(result
        .errors
        .iter()
        .any(|e| e.kind == AnalyzeErrorKind::UnsupportedFeature));
}

#[test]
fn test_validate_agrees_with_analyze() {
    // Ensure validate() and analyze() produce the same errors for invalid workflows
    let mut task1 = make_raw_task("task1");
    add_with_ref(&mut task1, "data", "$nonexistent");
    add_depends_on(&mut task1, &["also_missing"]);

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1]);

    let validate_result = validate(&raw);
    let analyze_result = analyze(raw.clone());

    // Both should fail
    assert!(
        validate_result.is_err(),
        "Should fail but got: {:?}",
        validate_result.value
    );
    assert!(
        analyze_result.is_err(),
        "Should fail but got: {:?}",
        analyze_result.value
    );

    // Both should report the same error kinds
    let validate_kinds: Vec<_> = validate_result.errors.iter().map(|e| &e.kind).collect();
    let analyze_kinds: Vec<_> = analyze_result.errors.iter().map(|e| &e.kind).collect();
    assert_eq!(validate_kinds, analyze_kinds);
}

#[test]
fn test_validate_valid_with_bindings() {
    let mut task2 = make_raw_task("task2");
    add_with_ref(&mut task2, "data", "$task1");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
    let result = validate(&raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);
}

// ====================================================================
// MCP server validation
// ====================================================================

#[test]
fn test_analyze_mcp_stdio_server_requires_command() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);

    // Create an MCP server with no command (stdio transport is default)
    let mut mcp_config = RawMcpConfig::new();
    mcp_config.servers.insert(
        Spanned::new("broken".to_string(), make_span(10, 16)),
        Spanned::new(RawMcpServer::default(), make_span(20, 30)),
    );
    raw.mcp = Some(Spanned::new(mcp_config, make_span(5, 35)));

    let result = analyze(raw);
    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::MissingField);
    let msg = result.errors[0].message.to_lowercase();
    assert!(
        msg.contains("command"),
        "error message should mention command: {}",
        msg
    );
    assert!(
        msg.contains("broken"),
        "error message should mention server name: {}",
        msg
    );
}

#[test]
fn test_analyze_mcp_stdio_server_with_command_ok() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);

    let mut mcp_config = RawMcpConfig::new();
    mcp_config.servers.insert(
        Spanned::new("novanet".to_string(), make_span(10, 17)),
        Spanned::new(
            RawMcpServer::with_command("cargo run -p novanet-mcp"),
            make_span(20, 60),
        ),
    );
    raw.mcp = Some(Spanned::new(mcp_config, make_span(5, 65)));

    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);
}

#[test]
fn test_analyze_mcp_sse_server_without_command_ok() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);

    // SSE server does not need a command — it uses a URL
    let mut mcp_config = RawMcpConfig::new();
    mcp_config.servers.insert(
        Spanned::new("remote".to_string(), make_span(10, 16)),
        Spanned::new(
            RawMcpServer::with_url("http://localhost:8080"),
            make_span(20, 60),
        ),
    );
    raw.mcp = Some(Spanned::new(mcp_config, make_span(5, 65)));

    let result = analyze(raw);
    assert!(result.is_ok(), "SSE server should not cause error");
    // SSE server should produce a warning about being dropped
    assert!(
        !result.warnings.is_empty(),
        "SSE server should produce a warning"
    );
    assert!(
        result.warnings[0].message.contains("SSE"),
        "warning should mention SSE, got: {}",
        result.warnings[0].message
    );
}

#[test]
fn test_analyze_mcp_stdio_server_empty_command() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);

    // A server with an empty command string should also fail
    let mut mcp_config = RawMcpConfig::new();
    let server = RawMcpServer {
        command: Some(Spanned::new(String::new(), make_span(25, 25))),
        ..Default::default()
    };
    mcp_config.servers.insert(
        Spanned::new("empty_cmd".to_string(), make_span(10, 19)),
        Spanned::new(server, make_span(20, 30)),
    );
    raw.mcp = Some(Spanned::new(mcp_config, make_span(5, 35)));

    let result = analyze(raw);
    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::MissingField);
}

#[test]
fn test_analyze_mcp_from_config_accepted() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);
    let mut mcp_config = RawMcpConfig::new();
    mcp_config.servers.insert(
        Spanned::new("neo4j".to_string(), make_span(10, 15)),
        Spanned::new(RawMcpServer::with_from("config"), make_span(20, 40)),
    );
    raw.mcp = Some(Spanned::new(mcp_config, make_span(5, 45)));

    let result = analyze(raw);
    assert!(
        result.is_ok(),
        "from: config should be accepted: {:?}",
        result.errors
    );
    let wf = result.value.unwrap();
    let server = wf.mcp_servers.get("neo4j").unwrap();
    assert_eq!(server.from, Some(McpFromSource::Config));
}

#[test]
fn test_analyze_mcp_from_and_command_conflict() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);
    let mut mcp_config = RawMcpConfig::new();
    let mut server = RawMcpServer::with_from("config");
    server.command = Some(Spanned::new("npx".to_string(), make_span(30, 33)));
    mcp_config.servers.insert(
        Spanned::new("neo4j".to_string(), make_span(10, 15)),
        Spanned::new(server, make_span(20, 40)),
    );
    raw.mcp = Some(Spanned::new(mcp_config, make_span(5, 45)));

    let result = analyze(raw);
    assert!(result.is_err(), "from: + command: should conflict");
    assert!(
        result.errors[0].message.contains("both"),
        "error should mention both: {}",
        result.errors[0].message
    );
}

#[test]
fn test_analyze_mcp_unknown_from_source() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1")]);
    let mut mcp_config = RawMcpConfig::new();
    mcp_config.servers.insert(
        Spanned::new("neo4j".to_string(), make_span(10, 15)),
        Spanned::new(
            RawMcpServer::with_from("@supernovae/pkg"),
            make_span(20, 40),
        ),
    );
    raw.mcp = Some(Spanned::new(mcp_config, make_span(5, 45)));

    let result = analyze(raw);
    assert!(result.is_err(), "unknown from: should error");
    assert!(
        result.errors[0].message.contains("Unknown MCP source"),
        "error should mention unknown source: {}",
        result.errors[0].message
    );
}

#[test]
fn test_analyze_rejects_empty_tasks_array() {
    let raw = make_raw_workflow("nika/workflow@0.12", vec![]);
    let result = analyze(raw);
    assert!(result.is_err(), "empty tasks array should be rejected");
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::InvalidValue);
    assert!(
        result.errors[0].message.contains("empty"),
        "error should mention empty, got: {}",
        result.errors[0].message
    );
}

#[test]
fn test_retry_on_infer_no_warning() {
    use crate::ast::raw::RawRetryConfig;

    let mut task = make_raw_task("my_task");
    task.action = Some(RawTaskAction::Infer(Spanned::new(
        RawInferAction {
            prompt: Spanned::new("Generate something".to_string(), make_span(0, 18)),
            ..Default::default()
        },
        make_span(0, 50),
    )));
    task.retry = Some(Spanned::new(
        RawRetryConfig {
            max_attempts: Some(Spanned::new(Templatable::Value(3), make_span(0, 1))),
            delay_ms: Some(Spanned::new(Templatable::Value(1000), make_span(0, 4))),
            ..Default::default()
        },
        make_span(0, 30),
    ));

    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    raw.model = Some(Spanned::new("test-model".to_string(), make_span(0, 10)));
    let result = analyze(raw);

    // Should succeed with no warnings — retry is valid on all verbs
    assert!(
        result.is_ok(),
        "retry on infer should not be an error: {:?}",
        result.errors
    );
    let retry_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("retry"))
        .collect();
    assert!(
        retry_warnings.is_empty(),
        "retry on infer should NOT emit a warning, got: {:?}",
        retry_warnings
    );
}

#[test]
fn test_retry_on_exec_no_warning() {
    use crate::ast::raw::RawRetryConfig;

    let mut task = make_raw_task("my_exec");
    task.action = Some(RawTaskAction::Exec(Spanned::new(
        RawExecAction {
            command: Spanned::new("echo hello".to_string(), make_span(0, 10)),
            ..Default::default()
        },
        make_span(0, 50),
    )));
    task.retry = Some(Spanned::new(
        RawRetryConfig {
            max_attempts: Some(Spanned::new(Templatable::Value(3), make_span(0, 1))),
            ..Default::default()
        },
        make_span(0, 30),
    ));

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = analyze(raw);

    assert!(
        result.is_ok(),
        "retry on exec should succeed: {:?}",
        result.errors
    );
    let retry_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("retry"))
        .collect();
    assert!(
        retry_warnings.is_empty(),
        "retry on exec should NOT emit a warning, got: {:?}",
        retry_warnings
    );
}

#[test]
fn test_retry_on_invoke_no_warning() {
    use crate::ast::raw::RawRetryConfig;

    let mut task = make_raw_task("my_invoke");
    task.action = Some(RawTaskAction::Invoke(Spanned::new(
        RawInvokeAction {
            tool: Some(Spanned::new(
                "nika:dimensions".to_string(),
                make_span(0, 15),
            )),
            ..Default::default()
        },
        make_span(0, 50),
    )));
    task.retry = Some(Spanned::new(
        RawRetryConfig {
            max_attempts: Some(Spanned::new(Templatable::Value(3), make_span(0, 1))),
            ..Default::default()
        },
        make_span(0, 30),
    ));

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = analyze(raw);

    assert!(
        result.is_ok(),
        "retry on invoke should succeed: {:?}",
        result.errors
    );
    let retry_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("retry"))
        .collect();
    assert!(
        retry_warnings.is_empty(),
        "retry on invoke should NOT emit a warning, got: {:?}",
        retry_warnings
    );
}

#[test]
fn test_retry_on_fetch_no_warning() {
    use crate::ast::raw::RawRetryConfig;

    let mut task = make_raw_task("my_fetch");
    task.action = Some(RawTaskAction::Fetch(Box::new(Spanned::new(
        RawFetchAction {
            url: Spanned::new("https://example.com".to_string(), make_span(0, 20)),
            ..Default::default()
        },
        make_span(0, 50),
    ))));
    task.retry = Some(Spanned::new(
        RawRetryConfig {
            max_attempts: Some(Spanned::new(Templatable::Value(3), make_span(0, 1))),
            ..Default::default()
        },
        make_span(0, 30),
    ));

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = analyze(raw);

    // Should succeed with no warnings
    assert!(
        result.is_ok(),
        "retry on fetch should succeed: {:?}",
        result.errors
    );
    assert!(
        result.warnings.is_empty(),
        "retry on fetch should NOT emit a warning, got: {:?}",
        result.warnings
    );
}

// ====================================================================
// Task ID format validation
// ====================================================================

#[test]
fn test_analyze_empty_task_id() {
    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("")]);
    let result = analyze(raw);
    assert!(result.is_err(), "empty task ID should be rejected");
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::InvalidValue),
        "expected InvalidValue for empty task ID, got: {:?}",
        result.errors
    );
}

#[test]
fn test_analyze_task_id_with_spaces() {
    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("my task")]);
    let result = analyze(raw);
    assert!(result.is_err(), "task ID with spaces should be rejected");
}

#[test]
fn test_analyze_task_id_dollar_prefix() {
    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("$reserved")]);
    let result = analyze(raw);
    assert!(
        result.is_err(),
        "task ID starting with $ should be rejected"
    );
}

#[test]
fn test_analyze_valid_task_id_with_hyphens_underscores() {
    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("my-task_v2")]);
    let result = analyze(raw);
    assert!(
        result.is_ok(),
        "task ID with hyphens/underscores is valid: {:?}",
        result.errors
    );
}

#[test]
fn test_analyze_valid_task_id_with_dots() {
    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("seo.keyword")]);
    let result = analyze(raw);
    assert!(
        result.is_ok(),
        "task ID with dots is valid: {:?}",
        result.errors
    );
}

// ====================================================================
// Duplicate include prefix detection
// ====================================================================

#[test]
fn test_analyze_duplicate_include_prefix() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("main")]);
    let includes = vec![
        Spanned::new(
            RawIncludeSpec {
                path: Spanned::new("./lib1.nika.yaml".to_string(), make_span(0, 20)),
                prefix: Some(Spanned::new("seo_".to_string(), make_span(0, 4))),
                span: make_span(0, 30),
            },
            make_span(0, 30),
        ),
        Spanned::new(
            RawIncludeSpec {
                path: Spanned::new("./lib2.nika.yaml".to_string(), make_span(0, 20)),
                prefix: Some(Spanned::new("seo_".to_string(), make_span(0, 4))),
                span: make_span(0, 30),
            },
            make_span(0, 30),
        ),
    ];
    raw.include = Some(Spanned::new(includes, make_span(0, 100)));

    let result = analyze(raw);
    assert!(
        result.is_err(),
        "duplicate include prefix should be rejected"
    );
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("duplicate include prefix")),
        "error should mention duplicate prefix: {:?}",
        result.errors
    );
}

#[test]
fn test_analyze_distinct_include_prefixes_ok() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("main")]);
    let includes = vec![
        Spanned::new(
            RawIncludeSpec {
                path: Spanned::new("./lib1.nika.yaml".to_string(), make_span(0, 20)),
                prefix: Some(Spanned::new("seo_".to_string(), make_span(0, 4))),
                span: make_span(0, 30),
            },
            make_span(0, 30),
        ),
        Spanned::new(
            RawIncludeSpec {
                path: Spanned::new("./lib2.nika.yaml".to_string(), make_span(0, 20)),
                prefix: Some(Spanned::new("content_".to_string(), make_span(0, 8))),
                span: make_span(0, 30),
            },
            make_span(0, 30),
        ),
    ];
    raw.include = Some(Spanned::new(includes, make_span(0, 100)));

    let result = analyze(raw);
    assert!(
        result.is_ok(),
        "distinct prefixes should be accepted: {:?}",
        result.errors
    );
}

// ====================================================================
// Multi-error collection and self-cycle
// ====================================================================

#[test]
fn test_analyze_collects_all_errors() {
    // Workflow with 2 problems: duplicate task ID and unknown with: ref
    let mut task1a = make_raw_task("task1");
    add_with_ref(&mut task1a, "x", "$nonexistent");
    let task1b = make_raw_task("task1"); // duplicate

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1a, task1b]);
    let result = analyze(raw);
    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(
        result.errors.len() >= 2,
        "analyzer should collect all errors, got {}: {:?}",
        result.errors.len(),
        result.errors
    );
}

#[test]
fn test_analyze_self_cycle() {
    let mut task = make_raw_task("loop_task");
    add_depends_on(&mut task, &["loop_task"]);

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = analyze(raw);
    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency),
        "self-cycle should be detected: {:?}",
        result.errors
    );
}

#[test]
fn test_analyze_self_cycle_via_with() {
    // A task referencing itself via with: binding must be caught as a cycle
    let mut task = make_raw_task("self_ref");
    add_with_ref(&mut task, "data", "$self_ref");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = analyze(raw);
    assert!(result.is_err(), "Should fail but got: {:?}", result.value);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::CyclicDependency),
        "self-cycle via with: should be detected: {:?}",
        result.errors
    );
}

// ── BUG-1: context.files must be transferred from raw to analyzed ──

#[test]
fn test_context_files_transferred_to_analyzed() {
    let task = make_raw_task("greet");
    let mut raw = make_raw_workflow("nika/workflow@0.9", vec![task]);

    // Add context with two files
    let mut files = IndexMap::new();
    files.insert(
        Spanned::new("brand".to_string(), make_span(0, 5)),
        Spanned::new("./context/brand.md".to_string(), make_span(0, 18)),
    );
    files.insert(
        Spanned::new("persona".to_string(), make_span(0, 7)),
        Spanned::new("./context/persona.json".to_string(), make_span(0, 22)),
    );
    raw.context = Some(Spanned::new(
        RawContextConfig { files: Some(files) },
        make_span(0, 50),
    ));

    let result = analyze(raw);
    assert!(result.is_ok(), "analyze failed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    assert_eq!(
        workflow.context_files.len(),
        2,
        "expected 2 context files, got {}",
        workflow.context_files.len()
    );

    // Verify aliases and paths are preserved
    let aliases: Vec<&str> = workflow
        .context_files
        .iter()
        .filter_map(|cf| cf.alias.as_deref())
        .collect();
    assert!(aliases.contains(&"brand"), "missing 'brand' alias");
    assert!(aliases.contains(&"persona"), "missing 'persona' alias");

    let paths: Vec<&str> = workflow
        .context_files
        .iter()
        .map(|cf| cf.path.as_str())
        .collect();
    assert!(paths.contains(&"./context/brand.md"));
    assert!(paths.contains(&"./context/persona.json"));
}

#[test]
fn test_context_files_empty_when_no_context_block() {
    let task = make_raw_task("greet");
    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);

    let result = analyze(raw);
    assert!(result.is_ok(), "Should succeed: {:?}", result.errors);

    let workflow = result.value.unwrap();
    assert!(
        workflow.context_files.is_empty(),
        "context_files should be empty when no context: block"
    );
}

// ═══════════════════════════════════════════════════════════════
// PRESET VALIDATION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_analyze_valid_preset_reference() {
    let yaml = r#"
schema: "nika/workflow@0.12"
agents:
  assistant:
    system: "You are helpful"
    provider: mock
    model: mock-fast
tasks:
  - id: gen
    preset: assistant
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result.errors.is_empty(),
        "Valid preset reference should pass: {:?}",
        result.errors
    );
}

#[test]
fn test_analyze_preset_unknown_emits_error() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: gen
    preset: ghost
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        !result.errors.is_empty(),
        "Unknown preset should produce error"
    );
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.kind == AnalyzeErrorKind::InvalidValue),
        "Error should be InvalidValue (NIKA-144), got: {:?}",
        result.errors
    );
}

#[test]
fn test_analyze_preset_missing_agents_block() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: gen
    preset: summarizer
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        !result.errors.is_empty(),
        "Preset without agents: block should produce error"
    );
}

#[test]
fn test_analyze_preset_exempts_missing_model() {
    let yaml = r#"
schema: "nika/workflow@0.12"
agents:
  writer:
    system: "You write well"
    provider: mock
    model: mock-fast
tasks:
  - id: gen
    preset: writer
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result.errors.is_empty(),
        "Preset with model in agents: block should exempt missing model: {:?}",
        result.errors
    );
}

#[test]
fn test_analyze_multiple_tasks_with_presets() {
    let yaml = r#"
schema: "nika/workflow@0.12"
agents:
  summarizer:
    system: "Summarize"
    provider: mock
    model: mock-fast
  writer:
    system: "Write"
    provider: mock
    model: mock-fast
tasks:
  - id: summary
    preset: summarizer
    infer: "summarize"
  - id: article
    preset: writer
    depends_on: [summary]
    with:
      data: $summary
    infer: "write about {{with.data}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result.errors.is_empty(),
        "Multiple presets should work: {:?}",
        result.errors
    );
}

#[test]
fn test_context_budget_valid() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test
tasks:
  - id: constrained
    context_budget: 4000
    infer: "Summarize"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result.errors.is_empty(),
        "Valid budget: {:?}",
        result.errors
    );
    let wf = result.value.unwrap();
    let task = wf.get_task_by_name("constrained").unwrap();
    assert_eq!(task.context_budget, Some(Templatable::Value(4000)));
}

#[test]
fn test_context_budget_zero_rejected() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test
tasks:
  - id: bad
    context_budget: 0
    infer: "test"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("context_budget")),
        "Should reject 0: {:?}",
        result.errors
    );
}

#[test]
fn test_context_budget_exceeds_max_rejected() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test
tasks:
  - id: huge
    context_budget: 250000
    infer: "test"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("context_budget")),
        "Should reject 250000: {:?}",
        result.errors
    );
}

#[test]
fn test_concurrency_zero_rejected() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test
tasks:
  - id: loop_task
    for_each: ["a", "b", "c"]
    concurrency: 0
    infer: "process {{with.item}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("concurrency: 0 is invalid")),
        "Should reject concurrency: 0: {:?}",
        result.errors
    );
}

#[test]
fn test_concurrency_one_accepted() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test
tasks:
  - id: loop_task
    for_each: ["a", "b"]
    concurrency: 1
    infer: "process {{with.item}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        !result
            .errors
            .iter()
            .any(|e| e.message.contains("concurrency")),
        "concurrency: 1 should be valid: {:?}",
        result.errors
    );
}

#[test]
fn test_concurrency_zero_inside_for_each_object_rejected() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test
tasks:
  - id: loop_task
    for_each:
      items: ["a", "b", "c"]
      concurrency: 0
    infer: "process {{with.item}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("concurrency: 0 is invalid")),
        "Should reject concurrency: 0 inside for_each object form: {:?}",
        result.errors
    );
}

// =========================================================================
// Artifact collision detection tests
// =========================================================================

#[test]
fn artifact_collision_same_static_path_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: a
    infer: "test"
    artifact:
      path: output.md
  - id: b
    infer: "test"
    artifact:
      path: output.md
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("collides")),
        "Should warn on duplicate static artifact path: {:?}",
        result.warnings
    );
}

#[test]
fn artifact_collision_append_mode_no_warning() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: a
    infer: "test"
    artifact:
      path: log.txt
      mode: append
  - id: b
    infer: "test"
    artifact:
      path: log.txt
      mode: append
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        !result
            .warnings
            .iter()
            .any(|w| w.message.contains("collides")),
        "Should NOT warn when both tasks use mode: append: {:?}",
        result.warnings
    );
}

#[test]
fn artifact_collision_template_path_skipped() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: a
    infer: "test"
    artifact:
      path: "{{with.name}}.md"
  - id: b
    infer: "test"
    artifact:
      path: "{{with.name}}.md"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        !result
            .warnings
            .iter()
            .any(|w| w.message.contains("collides")),
        "Should NOT warn on template paths: {:?}",
        result.warnings
    );
}

#[test]
fn artifact_collision_mixed_mode_warns() {
    // One append + one overwrite on same path = real collision
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: a
    infer: "test"
    artifact:
      path: output.md
      mode: append
  - id: b
    infer: "test"
    artifact:
      path: output.md
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("collides")),
        "Should warn when append collides with overwrite: {:?}",
        result.warnings
    );
}

#[test]
fn test_analyze_when_field_preserved() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test
tasks:
  - id: conditional
    when: "{{inputs.run_this}}"
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result.errors.is_empty(),
        "when: field should be valid: {:?}",
        result.errors
    );
    let task = &result.value.unwrap().tasks[0];
    assert_eq!(
        task.when.as_deref(),
        Some("{{inputs.run_this}}"),
        "when: field must be preserved through analysis"
    );
}

#[test]
fn test_analyze_when_field_none_when_absent() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test
tasks:
  - id: unconditional
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    let task = &result.value.unwrap().tasks[0];
    assert!(
        task.when.is_none(),
        "when: should be None when not specified"
    );
}

// ====================================================================
// M1: Model name validation
// ====================================================================

#[test]
fn test_validate_unknown_model_without_provider_warns() {
    let mut task = make_raw_task("gen");
    task.action = Some(RawTaskAction::Infer(Spanned::new(
        RawInferAction {
            prompt: Spanned::new("hello".to_string(), make_span(0, 5)),
            ..Default::default()
        },
        make_span(0, 20),
    )));
    task.model = Some(Spanned::new("typo-model".to_string(), make_span(0, 10)));
    // No provider set — should warn about unrecognized model

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("not a recognized model")),
        "unknown model without provider should warn, got: {:?}",
        result.warnings
    );
}

#[test]
fn test_validate_unknown_model_with_explicit_provider_no_warning() {
    // model: llama-3.1-8b-instant is ambiguous (groq/meta both have llama)
    // but provider: groq is explicitly set — user knows what they're doing
    let mut task = make_raw_task("gen");
    task.action = Some(RawTaskAction::Infer(Spanned::new(
        RawInferAction {
            prompt: Spanned::new("hello".to_string(), make_span(0, 5)),
            ..Default::default()
        },
        make_span(0, 20),
    )));
    task.model = Some(Spanned::new(
        "llama-3.1-8b-instant".to_string(),
        make_span(0, 20),
    ));
    task.provider = Some(Spanned::new("groq".to_string(), make_span(0, 4)));

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = validate(&raw);
    let model_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("not a recognized model"))
        .collect();
    assert!(
        model_warnings.is_empty(),
        "explicit provider should suppress model warning, got: {:?}",
        model_warnings
    );
}

#[test]
fn test_validate_custom_org_model_no_warning() {
    let mut task = make_raw_task("gen");
    task.action = Some(RawTaskAction::Infer(Spanned::new(
        RawInferAction {
            prompt: Spanned::new("hello".to_string(), make_span(0, 5)),
            ..Default::default()
        },
        make_span(0, 20),
    )));
    task.model = Some(Spanned::new(
        "org/custom-model".to_string(),
        make_span(0, 16),
    ));
    task.provider = Some(Spanned::new("mock".to_string(), make_span(0, 4)));

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = validate(&raw);
    let model_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("not a recognized model"))
        .collect();
    assert!(
        model_warnings.is_empty(),
        "org/model format should not warn, got: {:?}",
        model_warnings
    );
}

#[test]
fn test_validate_known_model_no_warning() {
    let mut task = make_raw_task("gen");
    task.action = Some(RawTaskAction::Infer(Spanned::new(
        RawInferAction {
            prompt: Spanned::new("hello".to_string(), make_span(0, 5)),
            ..Default::default()
        },
        make_span(0, 20),
    )));
    task.model = Some(Spanned::new(
        "claude-sonnet-4-6".to_string(),
        make_span(0, 17),
    ));
    task.provider = Some(Spanned::new("anthropic".to_string(), make_span(0, 9)));

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = validate(&raw);
    let model_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("not a recognized model"))
        .collect();
    assert!(
        model_warnings.is_empty(),
        "known model should not warn, got: {:?}",
        model_warnings
    );
}

#[test]
fn test_validate_workflow_level_unknown_model_warns() {
    let task = make_raw_task("step1");
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    raw.model = Some(Spanned::new("bogus-model".to_string(), make_span(0, 11)));
    // No provider set — should warn

    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("not a recognized model")),
        "workflow-level unknown model should warn, got: {:?}",
        result.warnings
    );
}

#[test]
fn test_validate_workflow_level_unknown_model_with_provider_no_warning() {
    let task = make_raw_task("step1");
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    raw.model = Some(Spanned::new(
        "llama-3.1-8b-instant".to_string(),
        make_span(0, 20),
    ));
    raw.provider = Some(Spanned::new("groq".to_string(), make_span(0, 4)));

    let result = validate(&raw);
    let model_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("not a recognized model"))
        .collect();
    assert!(
        model_warnings.is_empty(),
        "workflow-level model with explicit provider should not warn, got: {:?}",
        model_warnings
    );
}

#[test]
fn test_validate_model_with_inherited_workflow_provider_no_warning() {
    // Task has model: llama-3.1-8b-instant but NO task-level provider.
    // Workflow has provider: groq → model is served by groq, no warning.
    let mut task = make_raw_task("gen");
    task.action = Some(RawTaskAction::Infer(Spanned::new(
        RawInferAction {
            prompt: Spanned::new("hello".to_string(), make_span(0, 5)),
            ..Default::default()
        },
        make_span(0, 20),
    )));
    task.model = Some(Spanned::new(
        "llama-3.1-8b-instant".to_string(),
        make_span(0, 20),
    ));
    // No task.provider — inherited from workflow

    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    raw.provider = Some(Spanned::new("groq".to_string(), make_span(0, 4)));

    let result = validate(&raw);
    let model_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("not a recognized model"))
        .collect();
    assert!(
        model_warnings.is_empty(),
        "inherited workflow provider should suppress task model warning, got: {:?}",
        model_warnings
    );
}

#[test]
fn test_validate_model_without_any_provider_warns() {
    // Task has exotic model, no task-level provider, no workflow-level provider.
    // Should warn about unrecognized model.
    let mut task = make_raw_task("gen");
    task.action = Some(RawTaskAction::Infer(Spanned::new(
        RawInferAction {
            prompt: Spanned::new("hello".to_string(), make_span(0, 5)),
            ..Default::default()
        },
        make_span(0, 20),
    )));
    task.model = Some(Spanned::new("exotic-llm-v99".to_string(), make_span(0, 14)));
    // No task.provider, no workflow provider

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("not a recognized model")),
        "no provider anywhere should warn about exotic model, got: {:?}",
        result.warnings
    );
}

// ====================================================================
// M2: Builtin tool name validation
// ====================================================================

#[test]
fn test_validate_unknown_builtin_tool_is_error() {
    use crate::ast::raw::RawInvokeAction;

    let mut task = make_raw_task("call");
    task.action = Some(RawTaskAction::Invoke(Spanned::new(
        RawInvokeAction {
            tool: Some(Spanned::new("nika:typo_tool".to_string(), make_span(0, 14))),
            ..Default::default()
        },
        make_span(0, 30),
    )));

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = validate(&raw);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("unknown builtin tool")),
        "unknown nika: tool should error, got: {:?}",
        result.errors
    );
}

#[test]
fn test_validate_unknown_builtin_tool_suggests_similar() {
    use crate::ast::raw::RawInvokeAction;

    let mut task = make_raw_task("call");
    task.action = Some(RawTaskAction::Invoke(Spanned::new(
        RawInvokeAction {
            tool: Some(Spanned::new("nika:slepe".to_string(), make_span(0, 10))),
            ..Default::default()
        },
        make_span(0, 30),
    )));

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = validate(&raw);
    let err = result
        .errors
        .iter()
        .find(|e| e.message.contains("unknown builtin tool"))
        .expect("should have unknown tool error");
    assert!(
        err.suggestion.as_ref().is_some_and(|s| s.contains("sleep")),
        "should suggest 'sleep' for 'slepe', got: {:?}",
        err.suggestion
    );
}

#[test]
fn test_validate_known_builtin_tool_no_error() {
    use crate::ast::raw::RawInvokeAction;

    let mut task = make_raw_task("call");
    task.action = Some(RawTaskAction::Invoke(Spanned::new(
        RawInvokeAction {
            tool: Some(Spanned::new("nika:jq".to_string(), make_span(0, 7))),
            ..Default::default()
        },
        make_span(0, 30),
    )));

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = validate(&raw);
    let tool_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| e.message.contains("unknown builtin tool"))
        .collect();
    assert!(
        tool_errors.is_empty(),
        "known builtin should not error, got: {:?}",
        tool_errors
    );
}

#[test]
fn test_validate_mcp_tool_not_checked_as_builtin() {
    use crate::ast::raw::RawInvokeAction;

    let mut task = make_raw_task("call");
    task.action = Some(RawTaskAction::Invoke(Spanned::new(
        RawInvokeAction {
            tool: Some(Spanned::new(
                "novanet::search".to_string(),
                make_span(0, 15),
            )),
            ..Default::default()
        },
        make_span(0, 30),
    )));

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = validate(&raw);
    let tool_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| e.message.contains("unknown builtin tool"))
        .collect();
    assert!(
        tool_errors.is_empty(),
        "MCP tool should not be checked as builtin, got: {:?}",
        tool_errors
    );
}

// M4: for_each concurrency hints are handled by lint L031 — not duplicated in analyzer.

// ====================================================================
// G1: Temperature range validation
// ====================================================================

#[test]
fn test_validate_temperature_out_of_range_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: gen
    infer:
      prompt: "hello"
      temperature: 5.0
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("temperature") && w.message.contains("outside")),
        "temperature 5.0 should warn, got: {:?}",
        result.warnings
    );
}

#[test]
fn test_validate_temperature_negative_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: gen
    infer:
      prompt: "hello"
      temperature: -1.0
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("temperature") && w.message.contains("outside")),
        "negative temperature should warn, got: {:?}",
        result.warnings
    );
}

#[test]
fn test_validate_temperature_valid_no_warning() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: gen
    infer:
      prompt: "hello"
      temperature: 0.7
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    let temp_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("temperature") && w.message.contains("outside"))
        .collect();
    assert!(
        temp_warnings.is_empty(),
        "valid temperature should not warn, got: {:?}",
        temp_warnings
    );
}

// ====================================================================
// G2: thinking_budget without extended_thinking
// ====================================================================

#[test]
fn test_validate_thinking_budget_without_extended_thinking_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: gen
    infer:
      prompt: "hello"
      thinking_budget: 10000
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("thinking_budget")
                && w.message.contains("extended_thinking")),
        "thinking_budget without extended_thinking should warn, got: {:?}",
        result.warnings
    );
}

#[test]
fn test_validate_thinking_budget_with_extended_thinking_no_warning() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: gen
    infer:
      prompt: "hello"
      extended_thinking: true
      thinking_budget: 10000
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    let thinking_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| {
            w.message.contains("thinking_budget") && w.message.contains("extended_thinking")
        })
        .collect();
    assert!(
        thinking_warnings.is_empty(),
        "thinking_budget with extended_thinking should not warn, got: {:?}",
        thinking_warnings
    );
}

// ====================================================================
// G3: Extract modes requiring selector
// ====================================================================

#[test]
fn test_validate_extract_selector_without_selector_errors() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: selector
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("extract: selector requires a selector:")),
        "extract: selector without selector: should error, got: {:?}",
        result.errors
    );
}

#[test]
fn test_validate_extract_jsonpath_without_selector_errors() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: api
    fetch:
      url: "https://api.example.com/data"
      extract: jsonpath
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("extract: jsonpath requires a selector:")),
        "extract: jsonpath without selector: should error, got: {:?}",
        result.errors
    );
}

#[test]
fn test_validate_extract_selector_with_selector_no_error() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: selector
      selector: "main article"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    let selector_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| e.message.contains("requires a selector"))
        .collect();
    assert!(
        selector_errors.is_empty(),
        "extract: selector with selector: should not error, got: {:?}",
        selector_errors
    );
}

#[test]
fn test_validate_extract_markdown_no_selector_no_error() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: markdown
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    let selector_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| e.message.contains("requires a selector"))
        .collect();
    assert!(
        selector_errors.is_empty(),
        "extract: markdown should not require selector, got: {:?}",
        selector_errors
    );
}

// ====================================================================
// M5: Misplaced LLM fields warning
// ====================================================================

#[test]
fn test_validate_misplaced_llm_fields_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: gen
    temperature: 0.7
    infer:
      prompt: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("temperature") && w.message.contains("ignored")),
        "task-level temperature with full form infer should warn, got: {:?}",
        result.warnings
    );
}

#[test]
fn test_validate_shorthand_infer_no_misplaced_warning() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: gen
    temperature: 0.7
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    let misplaced_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("ignored"))
        .collect();
    assert!(
        misplaced_warnings.is_empty(),
        "shorthand infer should not warn about misplaced fields, got: {:?}",
        misplaced_warnings
    );
}

// ====================================================================
// M6: Provider name did-you-mean
// ====================================================================

#[test]
fn test_validate_unknown_provider_with_suggestion() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: antrhopic
model: claude-sonnet-4-6
tasks:
  - id: gen
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    let provider_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| e.message.contains("unknown provider"))
        .collect();
    assert!(!provider_errors.is_empty(), "unknown provider should error");
    assert!(
        provider_errors[0]
            .suggestion
            .as_ref()
            .is_some_and(|s| s.contains("anthropic")),
        "should suggest 'anthropic', got: {:?}",
        provider_errors[0].suggestion
    );
}

#[test]
fn test_validate_known_provider_no_error() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: anthropic
model: claude-sonnet-4-6
tasks:
  - id: gen
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    let provider_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| e.message.contains("unknown provider"))
        .collect();
    assert!(
        provider_errors.is_empty(),
        "known provider should not error, got: {:?}",
        provider_errors
    );
}

#[test]
fn test_validate_provider_alias_no_error() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude
model: claude-sonnet-4-6
tasks:
  - id: gen
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    let provider_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| e.message.contains("unknown provider"))
        .collect();
    assert!(
        provider_errors.is_empty(),
        "provider alias should not error, got: {:?}",
        provider_errors
    );
}

// ====================================================================
// G5: Validate {{inputs.*}} refs against declared inputs
// ====================================================================

#[test]
fn test_validate_inputs_ref_undeclared_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
inputs:
  topic: "AI"
tasks:
  - id: gen
    infer: "Research {{inputs.topic}} and {{inputs.locale}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("inputs.locale") && w.message.contains("not declared")),
        "undeclared inputs.locale should warn, got warnings: {:?}",
        result.warnings
    );
}

#[test]
fn test_validate_inputs_ref_declared_no_warning() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
inputs:
  topic: "AI"
tasks:
  - id: gen
    infer: "Research {{inputs.topic}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    let input_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("inputs.") && w.message.contains("not declared"))
        .collect();
    assert!(
        input_warnings.is_empty(),
        "declared input should not warn, got: {:?}",
        input_warnings
    );
}

#[test]
fn test_validate_inputs_ref_no_inputs_block_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: gen
    infer: "Research {{inputs.topic}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("inputs.topic")
                && w.message.contains("No inputs are declared")),
        "inputs ref without inputs: block should warn, got: {:?}",
        result.warnings
    );
}

#[test]
fn test_validate_inputs_ref_with_transform_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
inputs:
  topic: "AI"
tasks:
  - id: gen
    infer: "Research {{inputs.missing_key | upper}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("inputs.missing_key")),
        "inputs ref with transform should still warn, got: {:?}",
        result.warnings
    );
}

// ====================================================================
// G6: Validate {{context.*}} refs against declared context files
// ====================================================================

#[test]
fn test_validate_context_ref_undeclared_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
context:
  files:
    readme: ./README.md
tasks:
  - id: gen
    infer: "Summarize {{context.readme}} and {{context.changelog}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("context.changelog")
                && w.message.contains("not declared")),
        "undeclared context.changelog should warn, got: {:?}",
        result.warnings
    );
}

#[test]
fn test_validate_context_ref_declared_no_warning() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
context:
  files:
    readme: ./README.md
tasks:
  - id: gen
    infer: "Summarize {{context.readme}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    let ctx_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("context.") && w.message.contains("not declared"))
        .collect();
    assert!(
        ctx_warnings.is_empty(),
        "declared context should not warn, got: {:?}",
        ctx_warnings
    );
}

#[test]
fn test_validate_context_ref_no_context_block_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: gen
    infer: "Summarize {{context.readme}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("context.readme")
                && w.message.contains("No context files are declared")),
        "context ref without context: block should warn, got: {:?}",
        result.warnings
    );
}

// ====================================================================
// G4: Validate vision content with.X refs against with: block
// ====================================================================

#[test]
fn test_validate_vision_content_with_ref_undeclared_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: analyze
    with:
      img: $import_step
    infer:
      prompt: "Describe"
      content:
        - type: image
          source: "{{with.img.hash}}"
        - type: text
          text: "Describe {{with.unknown_alias}} in detail"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("with.unknown_alias")
                && w.message.contains("vision content")),
        "undeclared with ref in vision content should warn, got: {:?}",
        result.warnings
    );
}

#[test]
fn test_validate_vision_content_with_ref_declared_no_warning() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: analyze
    with:
      img: $import_step
      caption: $caption_step
    infer:
      prompt: "Describe"
      content:
        - type: image
          source: "{{with.img.hash}}"
        - type: text
          text: "Caption context: {{with.caption}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    let vision_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.contains("vision content") && w.message.contains("not declared"))
        .collect();
    assert!(
        vision_warnings.is_empty(),
        "declared with refs in vision content should not warn, got: {:?}",
        vision_warnings
    );
}

#[test]
fn test_validate_vision_content_no_with_block_warns() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
tasks:
  - id: analyze
    infer:
      prompt: "Describe"
      content:
        - type: text
          text: "Analyze {{with.data}}"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = validate(&raw);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("with.data")
                && w.message.contains("No with: bindings")),
        "with ref in vision content without with: block should warn, got: {:?}",
        result.warnings
    );
}

// ====================================================================
// extract_template_refs unit tests
// ====================================================================

#[test]
fn test_extract_template_refs_basic() {
    let refs = extract_template_refs("Hello {{inputs.topic}}", "inputs");
    assert_eq!(refs, vec!["topic"]);
}

#[test]
fn test_extract_template_refs_multiple() {
    let refs =
        extract_template_refs("{{inputs.a}} and {{inputs.b}} and {{inputs.a}}", "inputs");
    assert_eq!(refs, vec!["a", "b"]); // deduped
}

#[test]
fn test_extract_template_refs_with_transform() {
    let refs = extract_template_refs("{{inputs.name | upper | trim}}", "inputs");
    assert_eq!(refs, vec!["name"]);
}

#[test]
fn test_extract_template_refs_with_whitespace() {
    let refs = extract_template_refs("{{ inputs.topic }}", "inputs");
    assert_eq!(refs, vec!["topic"]);
}

#[test]
fn test_extract_template_refs_ignores_other_namespace() {
    let refs = extract_template_refs("{{with.data}} and {{inputs.topic}}", "inputs");
    assert_eq!(refs, vec!["topic"]);
}

#[test]
fn test_extract_template_refs_nested_path() {
    let refs = extract_template_refs("{{with.data.nested.field}}", "with");
    assert_eq!(refs, vec!["data"]); // only first-level key
}

#[test]
fn test_extract_template_refs_no_match() {
    let refs = extract_template_refs("Hello world, no templates", "inputs");
    assert!(refs.is_empty());
}

// ====================================================================
// schedule: full AST pipeline tests
// ====================================================================

#[test]
fn schedule_valid_cron_parses() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test
schedule: "@daily"
tasks:
  - id: gen
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result.errors.is_empty(),
        "Valid @daily schedule should parse without errors: {:?}",
        result.errors
    );

    let workflow = result.value.unwrap();
    let sched = workflow.schedule.as_ref().expect("schedule should be Some");
    assert_eq!(sched.cron, "@daily");
}

#[test]
fn schedule_invalid_cron_produces_error() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test
schedule: "not-valid"
tasks:
  - id: gen
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        !result.errors.is_empty(),
        "Invalid schedule should produce an error"
    );
    let err = &result.errors[0];
    assert_eq!(
        err.kind,
        AnalyzeErrorKind::InvalidValue,
        "Error kind should be InvalidValue (NIKA-144), got {:?}",
        err.kind
    );
    assert!(
        err.message.contains("invalid schedule"),
        "Error message should contain 'invalid schedule', got: {}",
        err.message
    );
}

#[test]
fn schedule_object_form_parses() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test
schedule:
  cron: "0 9 * * 1-5"
  timezone: "Europe/Paris"
  overlap: "queue"
tasks:
  - id: gen
    infer: "hello"
"#;
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(
        result.errors.is_empty(),
        "Object-form schedule should parse without errors: {:?}",
        result.errors
    );

    let workflow = result.value.unwrap();
    let sched = workflow.schedule.as_ref().expect("schedule should be Some");
    assert_eq!(sched.cron, "0 9 * * 1-5");
    assert_eq!(sched.timezone.as_deref(), Some("Europe/Paris"));
    assert_eq!(sched.overlap, crate::ast::schedule::OverlapPolicy::Queue);
}

#[test]
fn schedule_field_not_rejected_as_unknown_key() {
    // Regression: ensure the raw parser does NOT emit NIKA-163 for schedule:
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test
schedule: "@daily"
tasks:
  - id: gen
    infer: "hello"
"#;
    // Raw parse should succeed (schedule is in known_workflow_keys)
    let raw = crate::ast::raw::parse(yaml, crate::source::FileId(0));
    assert!(
        raw.is_ok(),
        "schedule: field should NOT be rejected as unknown key (NIKA-163): {:?}",
        raw.err()
    );
}
