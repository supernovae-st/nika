//! DAG Validation - use: wiring validation (v0.1)
//!
//! Validates:
//! - use: wiring references (task exists, is upstream)
//! - Template refs match use: declarations
//! - Task ID format (snake_case)
//!
//! Error codes:
//! - NIKA-055: Invalid task ID format (non-snake_case)
//! - NIKA-080: use.alias references unknown task
//! - NIKA-081: use.alias references non-upstream task
//! - NIKA-082: use.alias creates self-reference
//! - NIKA-083: Template {{use.alias}} references undeclared alias

use rustc_hash::FxHashSet;

use crate::ast::{TaskAction, Workflow};
use crate::binding::{validate_refs, validate_task_id, WiringSpec};
use crate::error::NikaError;

use super::flow::FlowGraph;

/// Validate a workflow's use: wiring against the flow graph
pub fn validate_use_wiring(workflow: &Workflow, flow_graph: &FlowGraph) -> Result<(), NikaError> {
    // Zero-clone: use &str references instead of owned Strings
    let all_task_ids: FxHashSet<&str> = workflow.tasks.iter().map(|t| t.id.as_str()).collect();

    for task in &workflow.tasks {
        if let Some(ref wiring) = task.use_wiring {
            validate_wiring(&task.id, wiring, &all_task_ids, flow_graph)?;
        }

        // FIX: Validate that {{use.alias}} refs in templates match declared aliases
        validate_template_refs(task)?;
    }

    Ok(())
}

/// Validate that {{use.alias}} references in task templates match declared aliases
///
/// BUG FIX (2026-02-21): Previously validate_refs() existed but was never called.
/// Now it's called during `nika check` to catch template typos early.
fn validate_template_refs(task: &crate::ast::Task) -> Result<(), NikaError> {
    // Collect declared aliases from use: block
    let mut declared_aliases: FxHashSet<String> = task
        .use_wiring
        .as_ref()
        .map(|w| w.keys().cloned().collect())
        .unwrap_or_default();

    // BUG FIX (2026-02-21): Add for_each loop variable to declared aliases
    // If task has for_each, the loop variable (for_each_as) is a valid alias
    if task.for_each.is_some() {
        let loop_var = task.for_each_as.as_deref().unwrap_or("item");
        declared_aliases.insert(loop_var.to_string());
    }

    // If no use: block, {{use.alias}} refs are invalid (catch early)
    // Extract templates from the task action and validate each
    let templates = extract_templates_from_action(&task.action);

    for template in templates {
        validate_refs(&template, &declared_aliases, &task.id)?;
    }

    Ok(())
}

/// Extract all template strings from a task action
fn extract_templates_from_action(action: &TaskAction) -> Vec<String> {
    let mut templates = Vec::new();

    match action {
        TaskAction::Infer { infer } => {
            templates.push(infer.prompt.clone());
        }
        TaskAction::Exec { exec } => {
            templates.push(exec.command.clone());
        }
        TaskAction::Fetch { fetch } => {
            templates.push(fetch.url.clone());
            if let Some(ref body) = fetch.body {
                templates.push(body.clone());
            }
        }
        TaskAction::Invoke { invoke } => {
            // params can contain templates in values
            if let Some(params) = &invoke.params {
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

/// Recursively collect string values from JSON that might contain templates
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

/// Validate a single use: wiring
///
/// Unified validation for new syntax: `alias: task.path [?? default]`
/// Ensures:
/// 1. Source task ID is valid (snake_case)
/// 2. Source task exists in workflow
/// 3. Source is not self-reference
/// 4. Source task has path to current task
fn validate_wiring(
    task_id: &str,
    wiring: &WiringSpec,
    all_task_ids: &FxHashSet<&str>,
    flow_graph: &FlowGraph,
) -> Result<(), NikaError> {
    for (alias, entry) in wiring {
        // Extract task_id from the path (first segment before '.')
        let from_task = entry.task_id();

        // Validate the source task ID format (snake_case) - O(n) check
        validate_task_id(from_task)?;

        // Validate that the source task exists, is not self-referential, and is upstream
        validate_from_task(alias, from_task, task_id, all_task_ids, flow_graph)?;
    }

    Ok(())
}

/// Validate that from_task exists and is upstream
///
/// Checks in order:
/// 1. Task exists in workflow (O(1) hash lookup)
/// 2. Not self-reference (O(1) comparison)
/// 3. Has path from source to current task in DAG (O(V+E) BFS)
fn validate_from_task(
    alias: &str,
    from_task: &str,
    task_id: &str,
    all_task_ids: &FxHashSet<&str>,
    flow_graph: &FlowGraph,
) -> Result<(), NikaError> {
    // Check not self-reference first (cheapest O(1) check)
    if from_task == task_id {
        return Err(NikaError::UseCircularDep {
            alias: alias.to_string(),
            from_task: from_task.to_string(),
            task_id: task_id.to_string(),
        });
    }

    // Check task exists (O(1) hash lookup)
    if !all_task_ids.contains(from_task) {
        return Err(NikaError::UseUnknownTask {
            alias: alias.to_string(),
            from_task: from_task.to_string(),
            task_id: task_id.to_string(),
        });
    }

    // Check from_task has path to current task (O(V+E) BFS)
    if !flow_graph.has_path(from_task, task_id) {
        return Err(NikaError::UseNotUpstream {
            alias: alias.to_string(),
            from_task: from_task.to_string(),
            task_id: task_id.to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{FetchParams, InferParams, InvokeParams, Task};
    use crate::binding::UseEntry;
    use serde_json::json;

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: UseEntry.task_id() extraction
    // ─────────────────────────────────────────────────────────────
    // Tests path parsing from "task.field.subfield" format
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn entry_task_id_simple() {
        let entry = UseEntry::new("weather");
        assert_eq!(entry.task_id(), "weather");
    }

    #[test]
    fn entry_task_id_with_path() {
        let entry = UseEntry::new("weather.summary");
        assert_eq!(entry.task_id(), "weather");
    }

    #[test]
    fn entry_task_id_nested_path() {
        let entry = UseEntry::new("weather.data.temp.celsius");
        assert_eq!(entry.task_id(), "weather");
    }

    #[test]
    fn entry_task_id_with_default() {
        let entry = UseEntry::with_default("weather.summary", json!("N/A"));
        assert_eq!(entry.task_id(), "weather");
    }

    #[test]
    fn entry_task_id_lazy_binding() {
        let entry = UseEntry::new_lazy("fetch_data.result");
        assert_eq!(entry.task_id(), "fetch_data");
        assert!(entry.is_lazy());
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Task ID validation (snake_case)
    // ─────────────────────────────────────────────────────────────
    // Tests NIKA-055 validation: must be [a-z0-9_]+ format
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
        let result = validate_task_id("fetch-api");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-055"));
    }

    #[test]
    fn task_id_invalid_uppercase() {
        let result = validate_task_id("myTask");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-055"));
    }

    #[test]
    fn task_id_invalid_dot() {
        let result = validate_task_id("weather.api");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-055"));
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: FlowGraph construction
    // ─────────────────────────────────────────────────────────────
    // Tests building FlowGraph from workflow flows
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn flowgraph_empty_workflow() {
        let yaml = r#"
schema: nika/workflow@0.1
provider: claude
tasks: []
flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);
        let final_tasks = graph.get_final_tasks();
        assert_eq!(final_tasks.len(), 0);
    }

    #[test]
    fn flowgraph_single_task() {
        let yaml = r#"
schema: nika/workflow@0.1
provider: claude
tasks:
  - id: task1
    infer: "Test"
flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        assert_eq!(graph.get_dependencies("task1").len(), 0);
        assert_eq!(graph.get_successors("task1").len(), 0);
        assert!(graph.contains("task1"));
        let final_tasks = graph.get_final_tasks();
        assert_eq!(final_tasks.len(), 1);
        assert_eq!(final_tasks[0].as_ref(), "task1");
    }

    #[test]
    fn flowgraph_linear_chain() {
        let yaml = r#"
schema: nika/workflow@0.1
id: linear
tasks:
  - id: task1
    infer:
      prompt: "A"
  - id: task2
    infer:
      prompt: "B"
  - id: task3
    infer:
      prompt: "C"
flows:
  - source: task1
    target: task2
  - source: task2
    target: task3
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        // Verify dependencies
        assert_eq!(graph.get_dependencies("task1").len(), 0);
        assert_eq!(graph.get_dependencies("task2").len(), 1);
        assert_eq!(graph.get_dependencies("task3").len(), 1);

        // Verify successors
        assert_eq!(graph.get_successors("task1").len(), 1);
        assert_eq!(graph.get_successors("task2").len(), 1);
        assert_eq!(graph.get_successors("task3").len(), 0);

        // Verify final task
        let final_tasks = graph.get_final_tasks();
        assert_eq!(final_tasks.len(), 1);
        assert_eq!(final_tasks[0].as_ref(), "task3");
    }

    #[test]
    fn flowgraph_multiple_sources_to_target() {
        let yaml = r#"
schema: nika/workflow@0.1
id: multi_source
tasks:
  - id: task1
    infer:
      prompt: "A"
  - id: task2
    infer:
      prompt: "B"
  - id: task3
    infer:
      prompt: "C"
flows:
  - source: [task1, task2]
    target: task3
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        // task3 should have 2 dependencies
        let deps = graph.get_dependencies("task3");
        assert_eq!(deps.len(), 2);

        // task1 and task2 should each have 1 successor
        assert_eq!(graph.get_successors("task1").len(), 1);
        assert_eq!(graph.get_successors("task2").len(), 1);
    }

    #[test]
    fn flowgraph_source_to_multiple_targets() {
        let yaml = r#"
schema: nika/workflow@0.1
id: multi_target
tasks:
  - id: task1
    infer:
      prompt: "A"
  - id: task2
    infer:
      prompt: "B"
  - id: task3
    infer:
      prompt: "C"
flows:
  - source: task1
    target: [task2, task3]
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        // task1 should have 2 successors
        assert_eq!(graph.get_successors("task1").len(), 2);

        // task2 and task3 should each have 1 dependency
        assert_eq!(graph.get_dependencies("task2").len(), 1);
        assert_eq!(graph.get_dependencies("task3").len(), 1);

        // task2 and task3 should both be final tasks
        let final_tasks = graph.get_final_tasks();
        assert_eq!(final_tasks.len(), 2);
    }

    #[test]
    fn flowgraph_diamond_pattern() {
        let yaml = r#"
schema: nika/workflow@0.1
id: diamond
tasks:
  - id: task1
    infer:
      prompt: "A"
  - id: task2
    infer:
      prompt: "B"
  - id: task3
    infer:
      prompt: "C"
  - id: task4
    infer:
      prompt: "D"
flows:
  - source: task1
    target: [task2, task3]
  - source: [task2, task3]
    target: task4
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        // task4 should have 2 dependencies
        assert_eq!(graph.get_dependencies("task4").len(), 2);

        // Only task4 should be final
        let final_tasks = graph.get_final_tasks();
        assert_eq!(final_tasks.len(), 1);
        assert_eq!(final_tasks[0].as_ref(), "task4");
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Cycle detection
    // ─────────────────────────────────────────────────────────────
    // Tests NIKA-020: Cycle detection using three-color DFS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn cycle_detection_simple_cycle() {
        let yaml = r#"
schema: nika/workflow@0.1
id: cycle_simple
tasks:
  - id: task1
    infer:
      prompt: "A"
  - id: task2
    infer:
      prompt: "B"
flows:
  - source: task1
    target: task2
  - source: task2
    target: task1
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        let result = graph.detect_cycles();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-020"));
    }

    #[test]
    fn cycle_detection_three_node_cycle() {
        let yaml = r#"
schema: nika/workflow@0.1
id: cycle_three
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
flows:
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: a
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        let result = graph.detect_cycles();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-020"));
    }

    #[test]
    fn cycle_detection_self_loop() {
        let yaml = r#"
schema: nika/workflow@0.1
id: self_loop
tasks:
  - id: task1
    infer:
      prompt: "A"
flows:
  - source: task1
    target: task1
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        let result = graph.detect_cycles();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-020"));
    }

    #[test]
    fn cycle_detection_complex_cycle() {
        // A → B → C → D → B (cycle: B → C → D → B)
        let yaml = r#"
schema: nika/workflow@0.1
id: complex_cycle
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
  - id: d
    infer:
      prompt: "D"
flows:
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: d
  - source: d
    target: b
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        let result = graph.detect_cycles();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-020"));
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Path reachability (has_path)
    // ─────────────────────────────────────────────────────────────
    // Tests BFS path finding for dependency validation
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn has_path_direct_edge() {
        let yaml = r#"
schema: nika/workflow@0.1
id: path_test
tasks:
  - id: task1
    infer:
      prompt: "A"
  - id: task2
    infer:
      prompt: "B"
flows:
  - source: task1
    target: task2
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        assert!(graph.has_path("task1", "task2"));
        assert!(!graph.has_path("task2", "task1"));
    }

    #[test]
    fn has_path_indirect_path() {
        let yaml = r#"
schema: nika/workflow@0.1
id: path_indirect
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
flows:
  - source: a
    target: b
  - source: b
    target: c
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        assert!(graph.has_path("a", "c"));
        assert!(graph.has_path("a", "b"));
        assert!(graph.has_path("b", "c"));
        assert!(!graph.has_path("c", "a"));
    }

    #[test]
    fn has_path_same_node() {
        let yaml = r#"
schema: nika/workflow@0.1
id: path_same
tasks:
  - id: task1
    infer:
      prompt: "A"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        // A node has a path to itself
        assert!(graph.has_path("task1", "task1"));
    }

    #[test]
    fn has_path_no_path() {
        let yaml = r#"
schema: nika/workflow@0.1
id: path_none
tasks:
  - id: task1
    infer:
      prompt: "A"
  - id: task2
    infer:
      prompt: "B"
  - id: task3
    infer:
      prompt: "C"
flows:
  - source: task1
    target: task2
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        assert!(!graph.has_path("task1", "task3"));
        assert!(!graph.has_path("task2", "task3"));
    }

    #[test]
    fn has_path_diamond_pattern() {
        let yaml = r#"
schema: nika/workflow@0.1
id: diamond_path
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
  - id: d
    infer:
      prompt: "D"
flows:
  - source: a
    target: [b, c]
  - source: [b, c]
    target: d
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        // All nodes should reach d via different paths
        assert!(graph.has_path("a", "d"));
        assert!(graph.has_path("b", "d"));
        assert!(graph.has_path("c", "d"));

        // But not backwards
        assert!(!graph.has_path("d", "a"));
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Template reference validation
    // ─────────────────────────────────────────────────────────────
    // Tests extraction and validation of {{use.alias}} refs
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn validate_template_infer_with_use_alias() {
        let task = Task {
            id: "task2".to_string(),
            action: TaskAction::Infer {
                infer: InferParams {
                    prompt: "Generate based on {{use.data}}".to_string(),
                    provider: None,
                    model: None,
                },
            },
            use_wiring: Some({
                let mut map = rustc_hash::FxHashMap::default();
                map.insert("data".to_string(), UseEntry::new("task1.result"));
                map
            }),
            for_each: None,
            for_each_as: None,
            output: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
        };

        let result = validate_template_refs(&task);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_template_undeclared_alias() {
        let task = Task {
            id: "task2".to_string(),
            action: TaskAction::Infer {
                infer: InferParams {
                    prompt: "Generate based on {{use.missing}}".to_string(),
                    provider: None,
                    model: None,
                },
            },
            use_wiring: Some({
                let mut map = rustc_hash::FxHashMap::default();
                map.insert("data".to_string(), UseEntry::new("task1.result"));
                map
            }),
            for_each: None,
            for_each_as: None,
            output: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
        };

        let result = validate_template_refs(&task);
        assert!(result.is_err());
        // validate_refs returns UnknownAlias error (NIKA-071) for undeclared aliases
        assert!(result.unwrap_err().to_string().contains("NIKA-071"));
    }

    #[test]
    fn validate_template_for_each_loop_variable() {
        let task = Task {
            id: "task1".to_string(),
            action: TaskAction::Infer {
                infer: InferParams {
                    prompt: "Process {{use.item}}".to_string(),
                    provider: None,
                    model: None,
                },
            },
            use_wiring: None,
            for_each: Some(json!(["a", "b"])),
            for_each_as: Some("item".to_string()),
            output: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
        };

        let result = validate_template_refs(&task);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_template_fetch_with_url_placeholder() {
        let task = Task {
            id: "task2".to_string(),
            action: TaskAction::Fetch {
                fetch: FetchParams {
                    url: "https://api.example.com/{{use.entity}}".to_string(),
                    method: "GET".to_string(),
                    headers: rustc_hash::FxHashMap::default(),
                    body: None,
                },
            },
            use_wiring: Some({
                let mut map = rustc_hash::FxHashMap::default();
                map.insert("entity".to_string(), UseEntry::new("task1.data.id"));
                map
            }),
            for_each: None,
            for_each_as: None,
            output: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
        };

        let result = validate_template_refs(&task);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_template_invoke_with_json_params() {
        let task = Task {
            id: "task2".to_string(),
            action: TaskAction::Invoke {
                invoke: InvokeParams {
                    mcp: "server_name".to_string(),
                    tool: Some("tool_name".to_string()),
                    params: Some(json!({
                        "entity": "{{use.entity_key}}",
                        "locale": "{{use.locale}}"
                    })),
                    resource: None,
                },
            },
            use_wiring: Some({
                let mut map = rustc_hash::FxHashMap::default();
                map.insert("entity_key".to_string(), UseEntry::new("task1.entity.key"));
                map.insert("locale".to_string(), UseEntry::new("task1.locale"));
                map
            }),
            for_each: None,
            for_each_as: None,
            output: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
        };

        let result = validate_template_refs(&task);
        assert!(result.is_ok());
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Full workflow wiring validation
    // ─────────────────────────────────────────────────────────────
    // Tests validate_use_wiring() end-to-end
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn validate_wiring_valid_upstream() {
        let yaml = r#"
schema: nika/workflow@0.1
id: valid_wiring
tasks:
  - id: task1
    infer:
      prompt: "Generate"
  - id: task2
    infer:
      prompt: "Use {{use.data}}"
    use:
      data: task1.result
flows:
  - source: task1
    target: task2
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let flow_graph = FlowGraph::from_workflow(&workflow);

        let result = validate_use_wiring(&workflow, &flow_graph);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_wiring_unknown_task() {
        let yaml = r#"
schema: nika/workflow@0.1
id: unknown_task
tasks:
  - id: task1
    infer:
      prompt: "Generate"
  - id: task2
    infer:
      prompt: "Use {{use.data}}"
    use:
      data: nonexistent.result
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let flow_graph = FlowGraph::from_workflow(&workflow);

        let result = validate_use_wiring(&workflow, &flow_graph);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-080"));
    }

    #[test]
    fn validate_wiring_self_reference() {
        let yaml = r#"
schema: nika/workflow@0.1
id: self_ref
tasks:
  - id: task1
    infer:
      prompt: "Use {{use.self}}"
    use:
      self: task1.result
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let flow_graph = FlowGraph::from_workflow(&workflow);

        let result = validate_use_wiring(&workflow, &flow_graph);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-082"));
    }

    #[test]
    fn validate_wiring_not_upstream() {
        let yaml = r#"
schema: nika/workflow@0.1
id: not_upstream
tasks:
  - id: task1
    infer:
      prompt: "A"
  - id: task2
    infer:
      prompt: "B"
  - id: task3
    infer:
      prompt: "Use {{use.data}}"
    use:
      data: task2.result
flows:
  - source: task1
    target: task3
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let flow_graph = FlowGraph::from_workflow(&workflow);

        let result = validate_use_wiring(&workflow, &flow_graph);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-081"));
    }

    #[test]
    fn validate_wiring_multiple_dependencies() {
        let yaml = r#"
schema: nika/workflow@0.1
id: multi_dep
tasks:
  - id: task1
    infer:
      prompt: "A"
  - id: task2
    infer:
      prompt: "B"
  - id: task3
    infer:
      prompt: "Combine {{use.a}} and {{use.b}}"
    use:
      a: task1.result
      b: task2.result
flows:
  - source: [task1, task2]
    target: task3
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let flow_graph = FlowGraph::from_workflow(&workflow);

        let result = validate_use_wiring(&workflow, &flow_graph);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_wiring_indirect_dependency() {
        let yaml = r#"
schema: nika/workflow@0.1
id: indirect_dep
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "Use {{use.data}}"
    use:
      data: a.result
flows:
  - source: a
    target: b
  - source: b
    target: c
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let flow_graph = FlowGraph::from_workflow(&workflow);

        // task1 → task3 via task2, so task1 is upstream of task3
        let result = validate_use_wiring(&workflow, &flow_graph);
        assert!(result.is_ok());
    }
}
