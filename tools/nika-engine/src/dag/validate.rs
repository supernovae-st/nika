//! DAG Validation — with: binding validation
//!
//! Validates:
//! - with: binding references (task exists, is upstream)
//! - Template refs match with: declarations
//! - Task ID format (snake_case)
//!
//! Error codes:
//! - NIKA-055: Invalid task ID format (non-snake_case)
//! - NIKA-080: with.alias references unknown task
//! - NIKA-081: with.alias references non-upstream task
//! - NIKA-082: with.alias creates self-reference
//!
//! Tests use programmatic AnalyzedWorkflow construction.

use rustc_hash::FxHashSet;

use crate::ast::analyzed::{AnalyzedTask, AnalyzedTaskAction, AnalyzedWorkflow};
use crate::binding::{validate_task_id, validate_with_refs, WithSpec};
use crate::error::NikaError;

use super::flow::Dag;

// Imports for runtime Workflow validation (validate_bindings)
use crate::ast::{TaskAction, Workflow};

/// Validate a workflow's with: bindings against the flow graph.
///
/// For each task that has a `with_spec`, validates:
/// 1. Each binding that references a task (Context/Input/Env/LoopVar are skipped)
///    - The referenced task exists in the workflow
///    - The referenced task is not a self-reference
///    - The referenced task is upstream (reachable via DAG edges)
/// 2. Template `{{with.alias}}` refs match declared aliases
pub fn validate_with_bindings(
    workflow: &AnalyzedWorkflow,
    flow_graph: &Dag,
) -> Result<(), NikaError> {
    let all_task_ids: FxHashSet<&str> = workflow.tasks.iter().map(|t| t.name.as_str()).collect();

    for task in &workflow.tasks {
        if !task.with_spec.is_empty() {
            let loop_var = task.for_each.as_ref().map(|fe| fe.as_var.as_str());
            validate_with_spec(
                &task.name,
                &task.with_spec,
                &all_task_ids,
                flow_graph,
                loop_var,
            )?;
        }

        validate_template_refs(task)?;
    }

    Ok(())
}

/// Validate that `{{with.alias}}` references in task templates match declared aliases.
fn validate_template_refs(task: &AnalyzedTask) -> Result<(), NikaError> {
    let mut declared_aliases: FxHashSet<String> = task.with_spec.keys().cloned().collect();

    // Add for_each loop variable and index to declared aliases
    if let Some(ref for_each) = task.for_each {
        declared_aliases.insert(for_each.as_var.clone());
        declared_aliases.insert("for_each_index".to_string());
    }

    // Extract templates from the task action and validate each
    let templates = extract_templates_from_action(&task.action);

    for template in templates {
        validate_with_refs(&template, &declared_aliases, &task.name)?;
    }

    Ok(())
}

/// Extract all template strings from an analyzed task action.
fn extract_templates_from_action(action: &AnalyzedTaskAction) -> Vec<String> {
    let mut templates = Vec::new();

    match action {
        AnalyzedTaskAction::Infer(infer) => {
            templates.push(infer.prompt.clone());
            if let Some(ref system) = infer.system {
                templates.push(system.clone());
            }
            // Vision content: text parts may contain templates
            if let Some(ref parts) = infer.content {
                for part in parts {
                    if let nika_core::ast::content::AnalyzedContentPart::Text { text } = part {
                        templates.push(text.clone());
                    }
                }
            }
        }
        AnalyzedTaskAction::Exec(exec) => {
            templates.push(exec.command.clone());
            if let Some(ref cwd) = exec.cwd {
                templates.push(cwd.clone());
            }
            for value in exec.env.values() {
                templates.push(value.clone());
            }
        }
        AnalyzedTaskAction::Fetch(fetch) => {
            templates.push(fetch.url.clone());
            if let Some(ref body) = fetch.body {
                templates.push(body.clone());
            }
            for value in fetch.headers.values() {
                templates.push(value.clone());
            }
            if let Some(ref json) = fetch.json {
                collect_string_values(json, &mut templates);
            }
        }
        AnalyzedTaskAction::Invoke(invoke) => {
            if let Some(params) = &invoke.params {
                collect_string_values(params, &mut templates);
            }
        }
        AnalyzedTaskAction::Agent(agent) => {
            templates.push(agent.prompt.clone());
            if let Some(ref system) = agent.system {
                templates.push(system.clone());
            }
        }
    }

    templates
}

/// Recursively collect string values from JSON that might contain templates.
fn collect_string_values(value: &serde_json::Value, templates: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => {
            templates.push(s.clone());
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                collect_string_values(item, templates);
            }
        }
        serde_json::Value::Object(obj) => {
            for v in obj.values() {
                collect_string_values(v, templates);
            }
        }
        _ => {}
    }
}

/// Validate a single with: spec against the DAG.
///
/// For each binding entry that references a task:
/// 1. Validates the source task ID format (snake_case)
/// 2. Validates the source task exists
/// 3. Validates the source is not a self-reference
/// 4. Validates the source task is upstream (reachable via DAG path)
///
/// Non-task bindings (Context, Input, Env, LoopVar) are silently skipped.
fn validate_with_spec(
    task_id: &str,
    with_spec: &WithSpec,
    all_task_ids: &FxHashSet<&str>,
    flow_graph: &Dag,
    loop_var: Option<&str>,
) -> Result<(), NikaError> {
    for (alias, entry) in with_spec {
        // Skip non-task bindings (Context/Input/Env/LoopVar return None)
        let Some(from_task) = entry.task_id() else {
            continue;
        };

        // E11: detect for_each loop variable used as task reference in with:
        if let Some(var) = loop_var {
            if from_task == var {
                return Err(NikaError::WithUnknownTask {
                    alias: alias.to_string(),
                    from_task: format!(
                        "{from_task} (this is a for_each loop variable, not a task — \
                         access it as {{{{with.{var}}}}} in templates instead)"
                    ),
                    task_id: task_id.to_string(),
                });
            }
        }

        // Validate the source task ID format (snake_case)
        validate_task_id(from_task)?;

        // Validate that the source task exists, is not self-referential, and is upstream
        validate_from_task(alias, from_task, task_id, all_task_ids, flow_graph)?;
    }

    Ok(())
}

/// Validate that from_task exists and is upstream.
///
/// Checks in order (cheapest first):
/// 1. Not self-reference (O(1) comparison)
/// 2. Task exists in workflow (O(1) hash lookup)
/// 3. Has path from source to current task in DAG (O(V+E) BFS)
fn validate_from_task(
    alias: &str,
    from_task: &str,
    task_id: &str,
    all_task_ids: &FxHashSet<&str>,
    flow_graph: &Dag,
) -> Result<(), NikaError> {
    // Check not self-reference first (cheapest O(1) check)
    if from_task == task_id {
        return Err(NikaError::WithCircularDep {
            alias: alias.to_string(),
            from_task: from_task.to_string(),
            task_id: task_id.to_string(),
        });
    }

    // Check task exists (O(1) hash lookup)
    if !all_task_ids.contains(from_task) {
        return Err(NikaError::WithUnknownTask {
            alias: alias.to_string(),
            from_task: from_task.to_string(),
            task_id: task_id.to_string(),
        });
    }

    // Check from_task has path to current task (O(V+E) BFS)
    if !flow_graph.has_path(from_task, task_id) {
        return Err(NikaError::WithNotUpstream {
            alias: alias.to_string(),
            from_task: from_task.to_string(),
            task_id: task_id.to_string(),
        });
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Workflow validation (runtime path)
// ═══════════════════════════════════════════════════════════════

/// Validate `Workflow` `with:` bindings against the DAG.
///
/// Used by the runtime runner, CLI check command, and TUI standalone.
/// New code targeting the analyzer pipeline should use `validate_with_bindings()`.
pub fn validate_bindings(workflow: &Workflow, flow_graph: &Dag) -> Result<(), NikaError> {
    let all_task_ids: FxHashSet<&str> = workflow.tasks.iter().map(|t| t.id.as_str()).collect();

    for task in &workflow.tasks {
        if let Some(ref with_spec) = task.with_spec {
            let loop_var = if task.for_each.is_some() {
                task.for_each_as.as_deref().or(Some("item"))
            } else {
                None
            };
            validate_with_spec(&task.id, with_spec, &all_task_ids, flow_graph, loop_var)?;
        }
        validate_template_refs_with(task)?;
    }

    Ok(())
}

/// Validate `{{with.alias}}` template refs (runtime `Task` type).
fn validate_template_refs_with(task: &crate::ast::Task) -> Result<(), NikaError> {
    let mut declared_aliases: FxHashSet<String> = task
        .with_spec
        .as_ref()
        .map(|w| w.keys().cloned().collect())
        .unwrap_or_default();

    // Add for_each loop variable and index
    if task.for_each.is_some() {
        let as_var = task
            .for_each_as
            .clone()
            .unwrap_or_else(|| "item".to_string());
        declared_aliases.insert(as_var);
        declared_aliases.insert("for_each_index".to_string());
    }

    let templates = extract_task_templates(&task.action);
    for template in templates {
        validate_with_refs(&template, &declared_aliases, &task.id)?;
    }

    Ok(())
}

/// Extract template strings from a runtime `TaskAction`.
fn extract_task_templates(action: &TaskAction) -> Vec<String> {
    let mut templates = Vec::new();
    match action {
        TaskAction::Infer { infer } => {
            templates.push(infer.prompt.clone());
            if let Some(ref system) = infer.system {
                templates.push(system.clone());
            }
        }
        TaskAction::Exec { exec } => {
            templates.push(exec.command.clone());
            if let Some(ref cwd) = exec.cwd {
                templates.push(cwd.clone());
            }
            if let Some(ref env) = exec.env {
                for value in env.values() {
                    templates.push(value.clone());
                }
            }
        }
        TaskAction::Fetch { fetch } => {
            templates.push(fetch.url.clone());
            if let Some(ref body) = fetch.body {
                templates.push(body.clone());
            }
            for value in fetch.headers.values() {
                templates.push(value.clone());
            }
            if let Some(ref json) = fetch.json {
                collect_string_values(json, &mut templates);
            }
        }
        TaskAction::Invoke { invoke } => {
            if let Some(ref params) = invoke.params {
                collect_string_values(params, &mut templates);
            }
        }
        TaskAction::Agent { agent } => {
            templates.push(agent.prompt.clone());
            if let Some(ref system) = agent.system {
                templates.push(system.clone());
            }
        }
    }
    templates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::analyzed::{
        AnalyzedAgentAction, AnalyzedExecAction, AnalyzedFetchAction, AnalyzedForEach,
        AnalyzedInferAction, AnalyzedInvokeAction, AnalyzedTaskAction, TaskId, TaskTable,
    };
    use crate::binding::types::{BindingPath, BindingSource, PathSegment};
    use crate::binding::WithEntry;
    use crate::source::Span;
    use serde_json::json;
    use std::sync::Arc;

    // ═══════════════════════════════════════════════════════════════
    // HELPERS — Programmatic AnalyzedWorkflow construction
    // ═══════════════════════════════════════════════════════════════

    /// Build an AnalyzedWorkflow from descriptors: (name, depends_on, implicit_deps)
    fn build_workflow(descriptors: &[(&str, &[&str], &[&str])]) -> AnalyzedWorkflow {
        let mut task_table = TaskTable::new();
        let mut tasks = Vec::new();

        for (name, _, _) in descriptors {
            task_table.insert(name);
        }

        for (name, depends_on_names, implicit_dep_names) in descriptors {
            let id = task_table.get_id(name).unwrap();
            let depends_on: Vec<TaskId> = depends_on_names
                .iter()
                .filter_map(|n| task_table.get_id(n))
                .collect();
            let implicit_deps: Vec<TaskId> = implicit_dep_names
                .iter()
                .filter_map(|n| task_table.get_id(n))
                .collect();

            tasks.push(AnalyzedTask {
                id,
                name: name.to_string(),
                description: None,
                action: AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
                provider: None,
                model: None,
                base_url: None,
                with_spec: WithSpec::default(),
                depends_on,
                implicit_deps,
                output: None,
                for_each: None,
                retry: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                record: None,
                context_budget: None,
                preset: None,
                routing: None,
                span: Span::dummy(),
            });
        }

        AnalyzedWorkflow {
            task_table,
            tasks,
            ..Default::default()
        }
    }

    /// Descriptor for building workflows with bindings in tests:
    /// (name, depends_on, implicit_deps, with_bindings)
    type BindingDescriptor<'a> = (
        &'a str,
        &'a [&'a str],
        &'a [&'a str],
        &'a [(&'a str, &'a str)],
    );

    /// Build an AnalyzedWorkflow where tasks have with: bindings.
    /// descriptors: (name, depends_on, implicit_deps, with_bindings)
    /// where with_bindings is &[(alias, source_task)]
    fn build_workflow_with_bindings(descriptors: &[BindingDescriptor<'_>]) -> AnalyzedWorkflow {
        let mut task_table = TaskTable::new();
        let mut tasks = Vec::new();

        for (name, _, _, _) in descriptors {
            task_table.insert(name);
        }

        for (name, depends_on_names, implicit_dep_names, bindings) in descriptors {
            let id = task_table.get_id(name).unwrap();
            let depends_on: Vec<TaskId> = depends_on_names
                .iter()
                .filter_map(|n| task_table.get_id(n))
                .collect();
            let implicit_deps: Vec<TaskId> = implicit_dep_names
                .iter()
                .filter_map(|n| task_table.get_id(n))
                .collect();

            let mut with_spec = WithSpec::default();
            for (alias, source_task) in *bindings {
                with_spec.insert(
                    alias.to_string(),
                    WithEntry::simple(BindingPath {
                        source: BindingSource::Task(Arc::from(*source_task)),
                        segments: vec![PathSegment::Field("result".into())],
                    }),
                );
            }

            tasks.push(AnalyzedTask {
                id,
                name: name.to_string(),
                description: None,
                action: AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
                provider: None,
                model: None,
                base_url: None,
                with_spec,
                depends_on,
                implicit_deps,
                output: None,
                for_each: None,
                retry: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                record: None,
                context_budget: None,
                preset: None,
                routing: None,
                span: Span::dummy(),
            });
        }

        AnalyzedWorkflow {
            task_table,
            tasks,
            ..Default::default()
        }
    }

    /// Build an AnalyzedWorkflow with a specific action on the last task.
    fn build_workflow_with_action(
        descriptors: &[(&str, &[&str], &[&str])],
        action_task: &str,
        action: AnalyzedTaskAction,
        with_spec: WithSpec,
    ) -> AnalyzedWorkflow {
        let mut workflow = build_workflow(descriptors);
        for task in &mut workflow.tasks {
            if task.name == action_task {
                task.action = action.clone();
                task.with_spec = with_spec.clone();
            }
        }
        workflow
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Task ID validation (snake_case)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn task_id_valid_simple() {
        assert!(validate_task_id("weather").is_ok());
    }

    #[test]
    fn task_id_valid_with_underscore() {
        assert!(validate_task_id("get_weather").is_ok());
        assert!(validate_task_id("fetch_api_data").is_ok());
    }

    #[test]
    fn task_id_valid_with_numbers() {
        assert!(validate_task_id("task123").is_ok());
        assert!(validate_task_id("step2").is_ok());
    }

    #[test]
    fn task_id_invalid_dash() {
        let err = validate_task_id("fetch-api").unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn task_id_invalid_uppercase() {
        let err = validate_task_id("myTask").unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    #[test]
    fn task_id_invalid_dot() {
        let err = validate_task_id("weather.api").unwrap_err();
        assert!(err.to_string().contains("NIKA-055"));
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: DAG construction (uses Dag::from_analyzed)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn flowgraph_empty_workflow() {
        let workflow = build_workflow(&[]);
        let graph = Dag::from_analyzed(&workflow).unwrap();
        let final_tasks = graph.get_final_tasks();
        assert_eq!(final_tasks.len(), 0);
    }

    #[test]
    fn flowgraph_single_task() {
        let workflow = build_workflow(&[("task1", &[], &[])]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        assert_eq!(graph.get_dependencies("task1").len(), 0);
        assert_eq!(graph.get_successors("task1").len(), 0);
        assert!(graph.contains("task1"));
        let final_tasks = graph.get_final_tasks();
        assert_eq!(final_tasks.len(), 1);
        assert_eq!(final_tasks[0].as_ref(), "task1");
    }

    #[test]
    fn flowgraph_linear_chain() {
        // task1 → task2 → task3 via depends_on
        let workflow = build_workflow(&[
            ("task1", &[], &[]),
            ("task2", &["task1"], &[]),
            ("task3", &["task2"], &[]),
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        assert_eq!(graph.get_dependencies("task1").len(), 0);
        assert_eq!(graph.get_dependencies("task2").len(), 1);
        assert_eq!(graph.get_dependencies("task3").len(), 1);

        assert_eq!(graph.get_successors("task1").len(), 1);
        assert_eq!(graph.get_successors("task2").len(), 1);
        assert_eq!(graph.get_successors("task3").len(), 0);

        let final_tasks = graph.get_final_tasks();
        assert_eq!(final_tasks.len(), 1);
        assert_eq!(final_tasks[0].as_ref(), "task3");
    }

    #[test]
    fn flowgraph_multiple_sources_to_target() {
        // task1,task2 → task3
        let workflow = build_workflow(&[
            ("task1", &[], &[]),
            ("task2", &[], &[]),
            ("task3", &["task1", "task2"], &[]),
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        assert_eq!(graph.get_dependencies("task3").len(), 2);
        assert_eq!(graph.get_successors("task1").len(), 1);
        assert_eq!(graph.get_successors("task2").len(), 1);
    }

    #[test]
    fn flowgraph_source_to_multiple_targets() {
        // task1 → task2, task1 → task3
        let workflow = build_workflow(&[
            ("task1", &[], &[]),
            ("task2", &["task1"], &[]),
            ("task3", &["task1"], &[]),
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        assert_eq!(graph.get_successors("task1").len(), 2);
        assert_eq!(graph.get_dependencies("task2").len(), 1);
        assert_eq!(graph.get_dependencies("task3").len(), 1);

        let final_tasks = graph.get_final_tasks();
        assert_eq!(final_tasks.len(), 2);
    }

    #[test]
    fn flowgraph_diamond_pattern() {
        // a → b,c → d
        let workflow = build_workflow(&[
            ("a", &[], &[]),
            ("b", &["a"], &[]),
            ("c", &["a"], &[]),
            ("d", &["b", "c"], &[]),
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        assert_eq!(graph.get_dependencies("d").len(), 2);
        let final_tasks = graph.get_final_tasks();
        assert_eq!(final_tasks.len(), 1);
        assert_eq!(final_tasks[0].as_ref(), "d");
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Cycle detection
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn cycle_detection_simple_cycle() {
        // task1 → task2 → task1 (mutual depends_on)
        let workflow = build_workflow(&[("task1", &["task2"], &[]), ("task2", &["task1"], &[])]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        let err = graph.detect_cycles().unwrap_err();
        assert!(err.to_string().contains("NIKA-020"));
    }

    #[test]
    fn cycle_detection_three_node_cycle() {
        // a → b → c → a
        let workflow =
            build_workflow(&[("a", &["c"], &[]), ("b", &["a"], &[]), ("c", &["b"], &[])]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        let err = graph.detect_cycles().unwrap_err();
        assert!(err.to_string().contains("NIKA-020"));
    }

    #[test]
    fn cycle_detection_self_loop_silently_filtered() {
        // Self-references in depends_on are silently filtered by Dag::from_analyzed
        // (defense-in-depth — analyzer should catch these first)
        let workflow = build_workflow(&[("task1", &["task1"], &[])]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        let result = graph.detect_cycles();
        assert!(
            result.is_ok(),
            "Self-deps are filtered out, no cycle exists"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Path reachability (has_path)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn has_path_direct_edge() {
        let workflow = build_workflow(&[("task1", &[], &[]), ("task2", &["task1"], &[])]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        assert!(graph.has_path("task1", "task2"));
        assert!(!graph.has_path("task2", "task1"));
    }

    #[test]
    fn has_path_indirect_path() {
        let workflow = build_workflow(&[("a", &[], &[]), ("b", &["a"], &[]), ("c", &["b"], &[])]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        assert!(graph.has_path("a", "c"));
        assert!(graph.has_path("a", "b"));
        assert!(graph.has_path("b", "c"));
        assert!(!graph.has_path("c", "a"));
    }

    #[test]
    fn has_path_same_node() {
        let workflow = build_workflow(&[("task1", &[], &[])]);
        let graph = Dag::from_analyzed(&workflow).unwrap();
        assert!(graph.has_path("task1", "task1"));
    }

    #[test]
    fn has_path_no_path() {
        let workflow = build_workflow(&[
            ("task1", &[], &[]),
            ("task2", &["task1"], &[]),
            ("task3", &[], &[]), // Disconnected
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        assert!(!graph.has_path("task1", "task3"));
        assert!(!graph.has_path("task2", "task3"));
    }

    #[test]
    fn has_path_diamond_pattern() {
        let workflow = build_workflow(&[
            ("a", &[], &[]),
            ("b", &["a"], &[]),
            ("c", &["a"], &[]),
            ("d", &["b", "c"], &[]),
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();

        assert!(graph.has_path("a", "d"));
        assert!(graph.has_path("b", "d"));
        assert!(graph.has_path("c", "d"));
        assert!(!graph.has_path("d", "a"));
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Template reference validation
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn validate_template_infer_with_alias() {
        let mut with_spec = WithSpec::default();
        with_spec.insert(
            "data".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task("task1".into()),
                segments: vec![PathSegment::Field("result".into())],
            }),
        );

        let workflow = build_workflow_with_action(
            &[("task1", &[], &[]), ("task2", &["task1"], &["task1"])],
            "task2",
            AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "Generate based on {{with.data}}".to_string(),
                ..Default::default()
            }),
            with_spec,
        );

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn validate_template_undeclared_alias() {
        let mut with_spec = WithSpec::default();
        with_spec.insert(
            "data".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task("task1".into()),
                segments: vec![PathSegment::Field("result".into())],
            }),
        );

        let workflow = build_workflow_with_action(
            &[("task1", &[], &[]), ("task2", &["task1"], &["task1"])],
            "task2",
            AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "Generate based on {{with.missing}}".to_string(),
                ..Default::default()
            }),
            with_spec,
        );

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let err = validate_with_bindings(&workflow, &graph).unwrap_err();
        // validate_with_refs returns UnknownAlias error (NIKA-071)
        assert!(err.to_string().contains("NIKA-071"));
    }

    #[test]
    fn validate_template_for_each_loop_variable() {
        let mut workflow = build_workflow(&[("task1", &[], &[])]);
        workflow.tasks[0].action = AnalyzedTaskAction::Infer(AnalyzedInferAction {
            prompt: "Process {{with.item}}".to_string(),
            ..Default::default()
        });
        workflow.tasks[0].for_each = Some(AnalyzedForEach {
            items: r#"["a", "b"]"#.to_string(),
            as_var: "item".to_string(),
            ..Default::default()
        });

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn validate_template_fetch_with_url_placeholder() {
        let mut with_spec = WithSpec::default();
        with_spec.insert(
            "entity".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task("task1".into()),
                segments: vec![
                    PathSegment::Field("data".into()),
                    PathSegment::Field("id".into()),
                ],
            }),
        );

        let workflow = build_workflow_with_action(
            &[("task1", &[], &[]), ("task2", &["task1"], &["task1"])],
            "task2",
            AnalyzedTaskAction::Fetch(AnalyzedFetchAction {
                url: "https://api.example.com/{{with.entity}}".to_string(),
                ..Default::default()
            }),
            with_spec,
        );

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn validate_template_invoke_with_json_params() {
        let mut with_spec = WithSpec::default();
        with_spec.insert(
            "entity_key".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task("task1".into()),
                segments: vec![
                    PathSegment::Field("entity".into()),
                    PathSegment::Field("key".into()),
                ],
            }),
        );
        with_spec.insert(
            "locale".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task("task1".into()),
                segments: vec![PathSegment::Field("locale".into())],
            }),
        );

        let workflow = build_workflow_with_action(
            &[("task1", &[], &[]), ("task2", &["task1"], &["task1"])],
            "task2",
            AnalyzedTaskAction::Invoke(AnalyzedInvokeAction {
                tool: "novanet_context".to_string(),
                params: Some(json!({
                    "entity": "{{with.entity_key}}",
                    "locale": "{{with.locale}}"
                })),
                ..Default::default()
            }),
            with_spec,
        );

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn validate_template_agent_goal() {
        let mut with_spec = WithSpec::default();
        with_spec.insert(
            "topic".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task("research".into()),
                segments: vec![PathSegment::Field("result".into())],
            }),
        );

        let workflow = build_workflow_with_action(
            &[
                ("research", &[], &[]),
                ("writer", &["research"], &["research"]),
            ],
            "writer",
            AnalyzedTaskAction::Agent(Box::new(AnalyzedAgentAction {
                prompt: "Write about {{with.topic}}".to_string(),
                ..Default::default()
            })),
            with_spec,
        );

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn validate_template_exec_command() {
        let mut with_spec = WithSpec::default();
        with_spec.insert(
            "file".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task("prepare".into()),
                segments: vec![PathSegment::Field("path".into())],
            }),
        );

        let workflow = build_workflow_with_action(
            &[("prepare", &[], &[]), ("build", &["prepare"], &["prepare"])],
            "build",
            AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: "cat {{with.file}}".to_string(),
                ..Default::default()
            }),
            with_spec,
        );

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn validate_template_infer_system_prompt() {
        let mut with_spec = WithSpec::default();
        with_spec.insert(
            "persona".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task("setup".into()),
                segments: vec![PathSegment::Field("persona".into())],
            }),
        );

        let workflow = build_workflow_with_action(
            &[("setup", &[], &[]), ("generate", &["setup"], &["setup"])],
            "generate",
            AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "Generate content".to_string(),
                system: Some("You are {{with.persona}}".to_string()),
                ..Default::default()
            }),
            with_spec,
        );

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Full workflow with: binding validation
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn validate_with_spec_valid_upstream() {
        // task1 → task2 with binding data: task1.result
        let workflow = build_workflow_with_bindings(&[
            ("task1", &[], &[], &[]),
            ("task2", &["task1"], &["task1"], &[("data", "task1")]),
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn validate_with_spec_unknown_task() {
        // task2 references nonexistent via with:
        let workflow = build_workflow_with_bindings(&[
            ("task1", &[], &[], &[]),
            ("task2", &[], &[], &[("data", "nonexistent")]),
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();
        let err = validate_with_bindings(&workflow, &graph).unwrap_err();
        assert!(err.to_string().contains("NIKA-080"));
    }

    #[test]
    fn validate_with_spec_self_reference() {
        // task1 references itself via with:
        let workflow =
            build_workflow_with_bindings(&[("task1", &[], &[], &[("self_ref", "task1")])]);
        let graph = Dag::from_analyzed(&workflow).unwrap();
        let err = validate_with_bindings(&workflow, &graph).unwrap_err();
        assert!(err.to_string().contains("NIKA-082"));
    }

    #[test]
    fn validate_with_spec_implicit_dependency_from_with() {
        // task3 references task2 via with:, and task2 → task3 via implicit_deps
        let workflow = build_workflow_with_bindings(&[
            ("task1", &[], &[], &[]),
            ("task2", &[], &[], &[]),
            ("task3", &["task1"], &["task2"], &[("data", "task2")]),
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(
            result.is_ok(),
            "with: should be validated against implicit deps (task2 → task3)"
        );
    }

    #[test]
    fn validate_with_spec_multiple_dependencies() {
        // task3 references task1 and task2
        let workflow = build_workflow_with_bindings(&[
            ("task1", &[], &[], &[]),
            ("task2", &[], &[], &[]),
            (
                "task3",
                &["task1", "task2"],
                &[],
                &[("a", "task1"), ("b", "task2")],
            ),
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn validate_with_spec_indirect_dependency() {
        // a → b → c, and c references a (a is upstream via indirect path)
        let workflow = build_workflow_with_bindings(&[
            ("a", &[], &[], &[]),
            ("b", &["a"], &[], &[]),
            ("c", &["b"], &[], &[("data", "a")]),
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn validate_with_spec_non_upstream_task() {
        // task1 and task2 are disconnected, task2 references task1
        // task1 is NOT upstream of task2 (no path)
        let workflow = build_workflow_with_bindings(&[
            ("task1", &[], &[], &[]),
            ("task2", &[], &[], &[("data", "task1")]),
        ]);
        let graph = Dag::from_analyzed(&workflow).unwrap();
        let err = validate_with_bindings(&workflow, &graph).unwrap_err();
        assert!(err.to_string().contains("NIKA-081"));
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Non-task bindings are skipped
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn validate_with_spec_context_binding_skipped() {
        // A context binding should be silently skipped (no task lookup)
        let mut workflow = build_workflow(&[("task1", &[], &[])]);
        workflow.tasks[0].with_spec.insert(
            "brand".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Context("files".into()),
                segments: vec![
                    PathSegment::Field("files".into()),
                    PathSegment::Field("brand".into()),
                ],
            }),
        );

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(
            result.is_ok(),
            "Context bindings should be silently skipped"
        );
    }

    #[test]
    fn validate_with_spec_input_binding_skipped() {
        let mut workflow = build_workflow(&[("task1", &[], &[])]);
        workflow.tasks[0].with_spec.insert(
            "name".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Input("user_name".into()),
                segments: vec![PathSegment::Field("user_name".into())],
            }),
        );

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Input bindings should be silently skipped");
    }

    #[test]
    fn validate_with_spec_env_binding_skipped() {
        let mut workflow = build_workflow(&[("task1", &[], &[])]);
        workflow.tasks[0].with_spec.insert(
            "api_key".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Env("API_KEY".into()),
                segments: vec![PathSegment::Field("API_KEY".into())],
            }),
        );

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Env bindings should be silently skipped");
    }

    #[test]
    fn validate_with_spec_mixed_bindings() {
        // task2 has both task and non-task bindings
        // Only the task binding should be validated
        let mut workflow =
            build_workflow(&[("task1", &[], &[]), ("task2", &["task1"], &["task1"])]);
        workflow.tasks[1].with_spec.insert(
            "data".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task("task1".into()),
                segments: vec![PathSegment::Field("result".into())],
            }),
        );
        workflow.tasks[1].with_spec.insert(
            "brand".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Context("files".into()),
                segments: vec![PathSegment::Field("brand".into())],
            }),
        );
        workflow.tasks[1].with_spec.insert(
            "user".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Input("user_name".into()),
                segments: vec![PathSegment::Field("name".into())],
            }),
        );

        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(
            result.is_ok(),
            "Mixed bindings: only task binding should be validated"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: extract_templates_from_action
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn extract_templates_infer() {
        let action = AnalyzedTaskAction::Infer(AnalyzedInferAction {
            prompt: "Hello {{with.name}}".to_string(),
            system: Some("You are {{with.persona}}".to_string()),
            ..Default::default()
        });
        let templates = extract_templates_from_action(&action);
        assert_eq!(templates.len(), 2);
        assert!(templates.contains(&"Hello {{with.name}}".to_string()));
        assert!(templates.contains(&"You are {{with.persona}}".to_string()));
    }

    #[test]
    fn extract_templates_exec() {
        let action = AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: "echo {{with.msg}}".to_string(),
            ..Default::default()
        });
        let templates = extract_templates_from_action(&action);
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0], "echo {{with.msg}}");
    }

    #[test]
    fn extract_templates_fetch() {
        let action = AnalyzedTaskAction::Fetch(AnalyzedFetchAction {
            url: "https://{{with.host}}/api".to_string(),
            body: Some("{\"key\": \"{{with.key}}\"}".to_string()),
            ..Default::default()
        });
        let templates = extract_templates_from_action(&action);
        assert_eq!(templates.len(), 2);
    }

    #[test]
    fn extract_templates_invoke_json() {
        let action = AnalyzedTaskAction::Invoke(AnalyzedInvokeAction {
            tool: "test_tool".to_string(),
            params: Some(json!({
                "key": "{{with.entity}}",
                "nested": {
                    "inner": "{{with.locale}}"
                },
                "arr": ["{{with.item}}", "static"]
            })),
            ..Default::default()
        });
        let templates = extract_templates_from_action(&action);
        assert_eq!(templates.len(), 4); // entity, locale, item, static
    }

    #[test]
    fn extract_templates_agent_goal() {
        let action = AnalyzedTaskAction::Agent(Box::new(AnalyzedAgentAction {
            prompt: "Research {{with.topic}}".to_string(),
            ..Default::default()
        }));
        let templates = extract_templates_from_action(&action);
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0], "Research {{with.topic}}");
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: No templates (pass-through)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn validate_no_templates_passes() {
        let workflow = build_workflow(&[("task1", &[], &[]), ("task2", &["task1"], &[])]);
        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn validate_empty_workflow_passes() {
        let workflow = build_workflow(&[]);
        let graph = Dag::from_analyzed(&workflow).unwrap();
        let result = validate_with_bindings(&workflow, &graph);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    // ═══════════════════════════════════════════════════════════════
    // Runtime extract_task_templates coverage
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn extract_templates_exec_includes_cwd_and_env() {
        use crate::ast::ExecParams;
        let mut env = rustc_hash::FxHashMap::default();
        env.insert("TOKEN".to_string(), "{{with.secret}}".to_string());
        let action = crate::ast::TaskAction::Exec {
            exec: ExecParams {
                command: "echo {{with.msg}}".to_string(),
                shell: None,
                timeout: None,
                cwd: Some("{{with.dir}}".to_string()),
                env: Some(env),
                max_stdout: None,
            },
        };
        let templates = extract_task_templates(&action);
        assert!(templates.contains(&"echo {{with.msg}}".to_string()));
        assert!(
            templates.contains(&"{{with.dir}}".to_string()),
            "exec.cwd should be extracted: {:?}",
            templates,
        );
        assert!(
            templates.contains(&"{{with.secret}}".to_string()),
            "exec.env values should be extracted: {:?}",
            templates,
        );
    }

    #[test]
    fn extract_templates_fetch_includes_headers_and_json() {
        use crate::ast::FetchParams;
        let mut headers = rustc_hash::FxHashMap::default();
        headers.insert(
            "Authorization".to_string(),
            "Bearer {{with.token}}".to_string(),
        );
        let action = crate::ast::TaskAction::Fetch {
            fetch: FetchParams {
                url: "https://api.example.com/{{with.path}}".to_string(),
                method: "POST".to_string(),
                headers,
                body: None,
                json: Some(serde_json::json!({"key": "{{with.val}}"})),
                timeout: None,
                retry: None,
                follow_redirects: None,
                response: None,
                extract: None,
                selector: None,
            },
        };
        let templates = extract_task_templates(&action);
        assert!(templates.contains(&"https://api.example.com/{{with.path}}".to_string()));
        assert!(
            templates.contains(&"Bearer {{with.token}}".to_string()),
            "fetch.headers values should be extracted: {:?}",
            templates,
        );
        assert!(
            templates.contains(&"{{with.val}}".to_string()),
            "fetch.json string values should be extracted: {:?}",
            templates,
        );
    }
}
