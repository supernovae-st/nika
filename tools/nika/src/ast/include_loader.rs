//! Include Loader - DAG Fusion at Parse Time (v0.17 Schema @0.9)
//!
//! Loads and merges external workflows into the main DAG.
//! Tasks from included workflows share the same DataStore as the parent.
//!
//! # Example
//!
//! ```yaml
//! include:
//!   # Filesystem path
//!   - path: ./lib/seo-tasks.nika.yaml
//!     prefix: seo_
//!   # Package reference (v0.17)
//!   - pkg: "@workflows/common"
//!     prefix: common_
//! ```
//!
//! # Features
//!
//! - Recursive include expansion (includes can include other workflows)
//! - Task ID prefixing to prevent collisions
//! - Flow dependency rewriting for prefixed IDs
//! - Circular include detection
//! - Package reference support (v0.17)

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use crate::ast::{Flow, FlowEndpoint, Task, Workflow};
use crate::error::NikaError;
use crate::registry::resolver;
use crate::serde_yaml;

/// Maximum depth for recursive includes (prevent infinite loops)
const MAX_INCLUDE_DEPTH: usize = 10;

/// Validate that a path stays within the project boundary
///
/// Prevents path traversal attacks where include paths could escape
/// the project directory using `../` or symlinks.
///
/// # Security
///
/// This is a security-critical function. The canonical path of the included
/// file must be under the canonical base path to prevent:
/// - Reading sensitive files outside project (e.g., `/etc/passwd`)
/// - Including files from parent directories
/// - Symlink attacks pointing outside project
fn validate_path_boundary(base_path: &Path, target_path: &Path) -> Result<(), NikaError> {
    let canonical_base = base_path
        .canonicalize()
        .unwrap_or_else(|_| base_path.to_path_buf());
    let canonical_target = target_path
        .canonicalize()
        .map_err(|e| NikaError::WorkflowNotFound {
            path: format!("{}: {}", target_path.display(), e),
        })?;

    if !canonical_target.starts_with(&canonical_base) {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Path traversal detected: '{}' is outside project boundary '{}'",
                target_path.display(),
                base_path.display()
            ),
        });
    }

    Ok(())
}

/// Expand all includes in a workflow
///
/// This function recursively loads included workflows and merges their tasks
/// and flows into the main workflow. Task IDs are prefixed according to the
/// include spec to prevent collisions.
///
/// # Arguments
///
/// * `workflow` - The workflow to expand
/// * `base_path` - Base directory for resolving relative include paths
///
/// # Returns
///
/// The workflow with all includes expanded and tasks merged
///
/// # Errors
///
/// Returns `NikaError` if:
/// - Include file not found
/// - Include file parse error
/// - Circular include detected
/// - Maximum include depth exceeded
pub fn expand_includes(workflow: Workflow, base_path: &Path) -> Result<Workflow, NikaError> {
    expand_includes_recursive(workflow, base_path, 0, &mut HashSet::new())
}

/// Recursive include expansion with depth tracking
fn expand_includes_recursive(
    mut workflow: Workflow,
    base_path: &Path,
    depth: usize,
    visited: &mut HashSet<String>,
) -> Result<Workflow, NikaError> {
    // Check depth limit
    if depth > MAX_INCLUDE_DEPTH {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Maximum include depth ({}) exceeded. Check for circular includes.",
                MAX_INCLUDE_DEPTH
            ),
        });
    }

    // No includes to process
    let includes = match workflow.include.take() {
        Some(includes) if !includes.is_empty() => includes,
        _ => return Ok(workflow),
    };

    // Process each include
    for include_spec in includes {
        // v0.17: Validate that exactly one of path/pkg is specified
        include_spec
            .validate()
            .map_err(|e| NikaError::ValidationError { reason: e })?;

        // Resolve the include path (filesystem or package)
        let include_path = if let Some(ref pkg) = include_spec.pkg {
            // Package reference - resolve via registry (v0.17)
            let resolved =
                resolver::resolve_package_path(pkg).map_err(|e| NikaError::WorkflowNotFound {
                    path: format!(
                        "Package not found: {}. Error: {}. Try: spn add {}",
                        pkg, e, pkg
                    ),
                })?;

            // Try different filenames based on package type (v0.17+)
            // @jobs packages use job.nika.yaml, others use workflow.nika.yaml
            let candidates = if pkg.starts_with("@jobs/") {
                vec!["job.nika.yaml", "workflow.nika.yaml"]
            } else {
                vec!["workflow.nika.yaml"]
            };

            let mut found_path = None;
            for filename in candidates {
                let candidate_path = resolved.path.join(filename);
                if candidate_path.exists() {
                    found_path = Some(candidate_path);
                    break;
                }
            }

            match found_path {
                Some(path) => path,
                None => {
                    let expected = if pkg.starts_with("@jobs/") {
                        "job.nika.yaml or workflow.nika.yaml"
                    } else {
                        "workflow.nika.yaml"
                    };
                    return Err(NikaError::WorkflowNotFound {
                        path: format!(
                            "Package {} exists but missing {} at {}",
                            pkg,
                            expected,
                            resolved.path.display()
                        ),
                    });
                }
            }
        } else if let Some(ref path) = include_spec.path {
            // Filesystem path (original behavior)
            let resolved_path = base_path.join(path);

            // Security: Validate path stays within project boundary
            validate_path_boundary(base_path, &resolved_path)?;

            resolved_path
        } else {
            // Should never happen due to validate() call above
            return Err(NikaError::ValidationError {
                reason: "Include spec must have either 'path' or 'pkg'".to_string(),
            });
        };

        let canonical_path = include_path
            .canonicalize()
            .unwrap_or_else(|_| include_path.clone());
        let path_str = canonical_path.to_string_lossy().to_string();

        // Check for circular includes
        if visited.contains(&path_str) {
            let display_ref = include_spec
                .pkg
                .as_deref()
                .or(include_spec.path.as_deref())
                .unwrap_or("unknown");
            return Err(NikaError::ValidationError {
                reason: format!("Circular include detected: {}", display_ref),
            });
        }
        visited.insert(path_str.clone());

        // Load the included workflow
        let included_workflow = load_included_workflow(&include_path)?;

        // Recursively expand includes in the included workflow
        let include_base = include_path.parent().unwrap_or(Path::new("."));
        let expanded_included =
            expand_includes_recursive(included_workflow, include_base, depth + 1, visited)?;

        // Merge tasks and flows with prefix
        merge_workflow(
            &mut workflow,
            expanded_included,
            include_spec.prefix.as_deref(),
        )?;

        // Remove from visited after processing (allows same file in different branches)
        visited.remove(&path_str);
    }

    Ok(workflow)
}

/// Load and parse an included workflow file
fn load_included_workflow(path: &Path) -> Result<Workflow, NikaError> {
    let content = std::fs::read_to_string(path).map_err(|e| NikaError::WorkflowNotFound {
        path: format!("{}: {}", path.display(), e),
    })?;

    let workflow: Workflow = serde_yaml::from_str(&content).map_err(|e| NikaError::ParseError {
        details: format!(
            "Failed to parse included workflow '{}': {}",
            path.display(),
            e
        ),
    })?;

    Ok(workflow)
}

/// Merge an included workflow into the main workflow
///
/// - Tasks are added with optional prefix
/// - Flows are rewritten to use prefixed IDs
/// - Skills are merged (main workflow skills take precedence)
/// - Other fields (agents, context) are NOT merged (use top-level only)
fn merge_workflow(
    main: &mut Workflow,
    included: Workflow,
    prefix: Option<&str>,
) -> Result<(), NikaError> {
    // Merge tasks with prefix
    for task in included.tasks {
        let prefixed_task = prefix_task(task, prefix);
        main.tasks.push(prefixed_task);
    }

    // Merge flows with prefix
    for flow in included.flows {
        let prefixed_flow = prefix_flow(flow, prefix);
        main.flows.push(prefixed_flow);
    }

    // Merge skills (main workflow skills take precedence)
    if let Some(included_skills) = included.skills {
        match main.skills.as_mut() {
            Some(main_skills) => {
                // Insert included skills that don't exist in main
                for (alias, skill_path) in included_skills {
                    main_skills.entry(alias).or_insert(skill_path);
                }
            }
            None => {
                // Main has no skills, use included skills
                main.skills = Some(included_skills);
            }
        }
    }

    Ok(())
}

/// Prefix a task ID and use_wiring references (works with Arc<Task>)
fn prefix_task(task: Arc<Task>, prefix: Option<&str>) -> Arc<Task> {
    match prefix {
        Some(prefix) if !prefix.is_empty() => {
            // Clone and modify task ID
            let mut new_task = (*task).clone();
            new_task.id = format!("{}{}", prefix, new_task.id);

            // Also prefix use_wiring references
            if let Some(ref mut wiring) = new_task.use_wiring {
                for entry in wiring.values_mut() {
                    entry.path = prefix_binding_path(&entry.path, prefix);
                }
            }

            Arc::new(new_task)
        }
        _ => task, // No prefix, return as-is
    }
}

/// Prefix the task ID part of a binding path (e.g., "task.field" -> "prefix_task.field")
fn prefix_binding_path(path: &str, prefix: &str) -> String {
    if let Some(dot_pos) = path.find('.') {
        // Path has field access: "task_id.field" -> "prefix_task_id.field"
        let task_id = &path[..dot_pos];
        let rest = &path[dot_pos..];
        format!("{}{}{}", prefix, task_id, rest)
    } else {
        // Path is just task_id: "task_id" -> "prefix_task_id"
        format!("{}{}", prefix, path)
    }
}

/// Prefix a FlowEndpoint (handles Single and Multiple variants)
fn prefix_endpoint(endpoint: FlowEndpoint, prefix: &str) -> FlowEndpoint {
    match endpoint {
        FlowEndpoint::Single(id) => FlowEndpoint::Single(format!("{}{}", prefix, id)),
        FlowEndpoint::Multiple(ids) => FlowEndpoint::Multiple(
            ids.into_iter()
                .map(|id| format!("{}{}", prefix, id))
                .collect(),
        ),
    }
}

/// Prefix flow dependencies (source/target)
fn prefix_flow(flow: Flow, prefix: Option<&str>) -> Flow {
    match prefix {
        Some(prefix) if !prefix.is_empty() => Flow {
            source: prefix_endpoint(flow.source, prefix),
            target: prefix_endpoint(flow.target, prefix),
        },
        _ => flow, // No prefix, return as-is
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::context::ContextConfig;
    use crate::ast::IncludeSpec;
    use rustc_hash::FxHashMap;
    use tempfile::TempDir;

    fn make_test_workflow() -> Workflow {
        Workflow {
            schema: "nika/workflow@0.9".to_string(),
            provider: "claude".to_string(),
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: None,
            skills: None,
            tasks: vec![],
            flows: vec![],
        }
    }

    #[test]
    fn test_expand_includes_no_includes() {
        let workflow = make_test_workflow();
        let result = expand_includes(workflow.clone(), Path::new(".")).unwrap();
        assert_eq!(result.tasks.len(), 0);
    }

    #[test]
    fn test_expand_includes_empty_includes() {
        let mut workflow = make_test_workflow();
        workflow.include = Some(vec![]);
        let result = expand_includes(workflow, Path::new(".")).unwrap();
        assert_eq!(result.tasks.len(), 0);
    }

    #[test]
    fn test_prefix_task() {
        use crate::ast::{InferParams, TaskAction};

        let task = Arc::new(Task {
            id: "generate".to_string(),
            use_wiring: None,
            output: None,
            decompose: None,
            for_each: None,
            for_each_as: None,
            concurrency: None,
            fail_fast: None,
            action: TaskAction::Infer {
                infer: InferParams {
                    prompt: "test".to_string(),
                    ..Default::default()
                },
            },
        });

        let prefixed = prefix_task(Arc::clone(&task), Some("seo_"));
        assert_eq!(prefixed.id, "seo_generate");

        let no_prefix = prefix_task(Arc::clone(&task), None);
        assert_eq!(no_prefix.id, "generate");

        let empty_prefix = prefix_task(task, Some(""));
        assert_eq!(empty_prefix.id, "generate");
    }

    #[test]
    fn test_prefix_binding_path() {
        // Simple task ID
        assert_eq!(prefix_binding_path("task1", "lib_"), "lib_task1");

        // Task ID with field access
        assert_eq!(
            prefix_binding_path("task1.result", "lib_"),
            "lib_task1.result"
        );

        // Task ID with nested field access
        assert_eq!(
            prefix_binding_path("task1.data.items", "lib_"),
            "lib_task1.data.items"
        );
    }

    #[test]
    fn test_prefix_task_with_use_wiring() {
        use crate::ast::{InferParams, TaskAction};
        use crate::binding::{UseEntry, WiringSpec};

        let mut wiring = WiringSpec::default();
        wiring.insert("data".to_string(), UseEntry::new("other_task.result"));
        wiring.insert("config".to_string(), UseEntry::new("config_task"));

        let task = Arc::new(Task {
            id: "processor".to_string(),
            use_wiring: Some(wiring),
            output: None,
            decompose: None,
            for_each: None,
            for_each_as: None,
            concurrency: None,
            fail_fast: None,
            action: TaskAction::Infer {
                infer: InferParams {
                    prompt: "test".to_string(),
                    ..Default::default()
                },
            },
        });

        let prefixed = prefix_task(task, Some("lib_"));
        assert_eq!(prefixed.id, "lib_processor");

        // Check use_wiring paths are prefixed
        let wiring = prefixed.use_wiring.as_ref().unwrap();
        assert_eq!(wiring.get("data").unwrap().path, "lib_other_task.result");
        assert_eq!(wiring.get("config").unwrap().path, "lib_config_task");
    }

    #[test]
    fn test_prefix_flow() {
        let flow = Flow {
            source: FlowEndpoint::Single("task1".to_string()),
            target: FlowEndpoint::Single("task2".to_string()),
        };

        let prefixed = prefix_flow(flow.clone(), Some("lib_"));
        assert!(matches!(&prefixed.source, FlowEndpoint::Single(s) if s == "lib_task1"));
        assert!(matches!(&prefixed.target, FlowEndpoint::Single(s) if s == "lib_task2"));

        let no_prefix = prefix_flow(flow, None);
        assert!(matches!(&no_prefix.source, FlowEndpoint::Single(s) if s == "task1"));
        assert!(matches!(&no_prefix.target, FlowEndpoint::Single(s) if s == "task2"));
    }

    #[test]
    fn test_prefix_endpoint_single() {
        let endpoint = FlowEndpoint::Single("task1".to_string());
        let prefixed = prefix_endpoint(endpoint, "lib_");
        assert!(matches!(prefixed, FlowEndpoint::Single(s) if s == "lib_task1"));
    }

    #[test]
    fn test_prefix_endpoint_multiple() {
        let endpoint = FlowEndpoint::Multiple(vec!["task1".to_string(), "task2".to_string()]);
        let prefixed = prefix_endpoint(endpoint, "lib_");
        if let FlowEndpoint::Multiple(ids) = prefixed {
            assert_eq!(ids, vec!["lib_task1", "lib_task2"]);
        } else {
            panic!("Expected Multiple variant");
        }
    }

    #[test]
    fn test_expand_includes_with_file() {
        let temp_dir = TempDir::new().unwrap();

        // Create included workflow
        let included_yaml = r#"
schema: nika/workflow@0.9
provider: claude
tasks:
  - id: helper
    infer: "Help with something"
"#;
        std::fs::write(temp_dir.path().join("helper.nika.yaml"), included_yaml).unwrap();

        // Create main workflow with include
        let mut workflow = make_test_workflow();
        workflow.include = Some(vec![IncludeSpec {
            path: Some("helper.nika.yaml".to_string()),
            pkg: None,
            prefix: None,
        }]);

        let result = expand_includes(workflow, temp_dir.path()).unwrap();
        assert_eq!(result.tasks.len(), 1);
        assert_eq!(result.tasks[0].id, "helper");
    }

    #[test]
    fn test_expand_includes_with_prefix() {
        let temp_dir = TempDir::new().unwrap();

        // Create included workflow with multiple tasks
        let included_yaml = r#"
schema: nika/workflow@0.9
provider: claude
tasks:
  - id: task1
    infer: "Task 1"
  - id: task2
    infer: "Task 2"
flows:
  - source: task1
    target: task2
"#;
        std::fs::write(temp_dir.path().join("lib.nika.yaml"), included_yaml).unwrap();

        let mut workflow = make_test_workflow();
        workflow.include = Some(vec![IncludeSpec {
            path: Some("lib.nika.yaml".to_string()),
            pkg: None,
            prefix: Some("lib_".to_string()),
        }]);

        let result = expand_includes(workflow, temp_dir.path()).unwrap();

        assert_eq!(result.tasks.len(), 2);
        assert_eq!(result.tasks[0].id, "lib_task1");
        assert_eq!(result.tasks[1].id, "lib_task2");

        assert_eq!(result.flows.len(), 1);
        assert!(matches!(&result.flows[0].source, FlowEndpoint::Single(s) if s == "lib_task1"));
        assert!(matches!(&result.flows[0].target, FlowEndpoint::Single(s) if s == "lib_task2"));
    }

    #[test]
    fn test_expand_includes_multiple() {
        let temp_dir = TempDir::new().unwrap();

        // Create two included workflows
        std::fs::write(
            temp_dir.path().join("a.nika.yaml"),
            r#"
schema: nika/workflow@0.9
provider: claude
tasks:
  - id: a_task
    infer: "A"
"#,
        )
        .unwrap();

        std::fs::write(
            temp_dir.path().join("b.nika.yaml"),
            r#"
schema: nika/workflow@0.9
provider: claude
tasks:
  - id: b_task
    infer: "B"
"#,
        )
        .unwrap();

        let mut workflow = make_test_workflow();
        workflow.include = Some(vec![
            IncludeSpec {
                path: Some("a.nika.yaml".to_string()),
                pkg: None,
                prefix: None,
            },
            IncludeSpec {
                path: Some("b.nika.yaml".to_string()),
                pkg: None,
                prefix: None,
            },
        ]);

        let result = expand_includes(workflow, temp_dir.path()).unwrap();
        assert_eq!(result.tasks.len(), 2);
    }

    #[test]
    fn test_expand_includes_recursive() {
        let temp_dir = TempDir::new().unwrap();

        // Create nested includes: main -> lib -> util
        std::fs::write(
            temp_dir.path().join("util.nika.yaml"),
            r#"
schema: nika/workflow@0.9
provider: claude
tasks:
  - id: util_task
    infer: "Utility"
"#,
        )
        .unwrap();

        std::fs::write(
            temp_dir.path().join("lib.nika.yaml"),
            r#"
schema: nika/workflow@0.9
provider: claude
include:
  - path: util.nika.yaml
    prefix: util_
tasks:
  - id: lib_task
    infer: "Library"
"#,
        )
        .unwrap();

        let mut workflow = make_test_workflow();
        workflow.include = Some(vec![IncludeSpec {
            path: Some("lib.nika.yaml".to_string()),
            pkg: None,
            prefix: None,
        }]);

        let result = expand_includes(workflow, temp_dir.path()).unwrap();

        // Should have: util_util_task (double prefixed) and lib_task
        assert_eq!(result.tasks.len(), 2);
        let task_ids: Vec<_> = result.tasks.iter().map(|t| t.id.as_str()).collect();
        assert!(task_ids.contains(&"util_util_task"));
        assert!(task_ids.contains(&"lib_task"));
    }

    #[test]
    fn test_expand_includes_circular_detection() {
        let temp_dir = TempDir::new().unwrap();

        // Create circular include: a -> b -> a
        std::fs::write(
            temp_dir.path().join("a.nika.yaml"),
            r#"
schema: nika/workflow@0.9
provider: claude
include:
  - path: b.nika.yaml
tasks: []
"#,
        )
        .unwrap();

        std::fs::write(
            temp_dir.path().join("b.nika.yaml"),
            r#"
schema: nika/workflow@0.9
provider: claude
include:
  - path: a.nika.yaml
tasks: []
"#,
        )
        .unwrap();

        let mut workflow = make_test_workflow();
        workflow.include = Some(vec![IncludeSpec {
            path: Some("a.nika.yaml".to_string()),
            pkg: None,
            prefix: None,
        }]);

        let result = expand_includes(workflow, temp_dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Circular include"));
    }

    #[test]
    fn test_expand_includes_file_not_found() {
        let temp_dir = TempDir::new().unwrap();

        let mut workflow = make_test_workflow();
        workflow.include = Some(vec![IncludeSpec {
            path: Some("nonexistent.nika.yaml".to_string()),
            pkg: None,
            prefix: None,
        }]);

        let result = expand_includes(workflow, temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_includes_preserves_main_workflow_fields() {
        let temp_dir = TempDir::new().unwrap();

        std::fs::write(
            temp_dir.path().join("lib.nika.yaml"),
            r#"
schema: nika/workflow@0.9
provider: openai
model: gpt-4
context:
  files:
    ignored: ./ignored.md
tasks:
  - id: lib_task
    infer: "Test"
"#,
        )
        .unwrap();

        let mut workflow = make_test_workflow();
        workflow.model = Some("claude-sonnet".to_string());
        workflow.context = Some(ContextConfig {
            files: {
                let mut m = FxHashMap::default();
                m.insert("main".to_string(), "./main.md".to_string());
                m
            },
            session: None,
        });
        workflow.include = Some(vec![IncludeSpec {
            path: Some("lib.nika.yaml".to_string()),
            pkg: None,
            prefix: None,
        }]);

        let result = expand_includes(workflow, temp_dir.path()).unwrap();

        // Main workflow fields should be preserved
        assert_eq!(result.provider, "claude");
        assert_eq!(result.model, Some("claude-sonnet".to_string()));

        // Context from main workflow (included context is NOT merged)
        let ctx = result.context.unwrap();
        assert!(ctx.files.contains_key("main"));
        assert!(!ctx.files.contains_key("ignored"));
    }

    #[test]
    fn test_expand_includes_path_traversal_detection() {
        let temp_dir = TempDir::new().unwrap();

        // Create a valid workflow file in a subdirectory
        let sub_dir = temp_dir.path().join("project");
        std::fs::create_dir(&sub_dir).unwrap();

        // Create a file outside the project directory
        let parent_file = temp_dir.path().join("secret.nika.yaml");
        std::fs::write(
            &parent_file,
            r#"
schema: nika/workflow@0.9
provider: claude
tasks:
  - id: secret
    infer: "Secret task"
"#,
        )
        .unwrap();

        // Try to include a file from parent directory using path traversal
        let mut workflow = make_test_workflow();
        workflow.include = Some(vec![IncludeSpec {
            path: Some("../secret.nika.yaml".to_string()),
            pkg: None,
            prefix: None,
        }]);

        let result = expand_includes(workflow, &sub_dir);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("Path traversal") || err_str.contains("outside project"),
            "Expected path traversal error, got: {}",
            err_str
        );
    }

    #[test]
    fn test_validate_path_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let project = temp_dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();

        // Create files
        let valid_file = project.join("valid.yaml");
        std::fs::write(&valid_file, "test").unwrap();

        let outside_file = temp_dir.path().join("outside.yaml");
        std::fs::write(&outside_file, "test").unwrap();

        // Valid path within boundary
        assert!(validate_path_boundary(&project, &valid_file).is_ok());

        // Invalid path outside boundary
        let result = validate_path_boundary(&project, &outside_file);
        assert!(result.is_err());
    }

    // ============================================================================
    // SKILL MERGING TESTS (v0.15.1 - Sub-Plan 1: Wire Existing Skills)
    // ============================================================================

    #[test]
    fn test_expand_includes_merges_skills_from_included_workflow() {
        let temp_dir = TempDir::new().unwrap();

        // Create included workflow with skills
        let included_yaml = r#"
schema: nika/workflow@0.9
provider: claude
skills:
  code_review: "./skills/code-review.md"
  testing: "./skills/testing.md"
tasks:
  - id: helper
    infer: "Help with something"
"#;
        std::fs::write(temp_dir.path().join("included.nika.yaml"), included_yaml).unwrap();

        // Create main workflow WITHOUT skills, but with include
        let mut workflow = make_test_workflow();
        workflow.include = Some(vec![IncludeSpec {
            path: Some("included.nika.yaml".to_string()),
            pkg: None,
            prefix: None,
        }]);

        // Expand includes - this should merge skills from included workflow
        let result = expand_includes(workflow, temp_dir.path()).unwrap();

        // ASSERT: Skills from included workflow should be present
        assert!(
            result.skills.is_some(),
            "Skills should be merged from included workflow"
        );
        let skills = result.skills.unwrap();
        assert_eq!(
            skills.len(),
            2,
            "Should have 2 skills from included workflow"
        );
        assert!(
            skills.contains_key("code_review"),
            "Should have code_review skill"
        );
        assert!(skills.contains_key("testing"), "Should have testing skill");
    }

    #[test]
    fn test_expand_includes_main_workflow_skills_override_included() {
        let temp_dir = TempDir::new().unwrap();

        // Create included workflow with skills
        let included_yaml = r#"
schema: nika/workflow@0.9
provider: claude
skills:
  shared: "./skills/included-shared.md"
  only_included: "./skills/only-included.md"
tasks:
  - id: helper
    infer: "Help"
"#;
        std::fs::write(temp_dir.path().join("included.nika.yaml"), included_yaml).unwrap();

        // Create main workflow WITH skills (including one that conflicts)
        let mut workflow = make_test_workflow();
        let mut main_skills = rustc_hash::FxHashMap::default();
        main_skills.insert("shared".to_string(), "./skills/main-shared.md".to_string());
        main_skills.insert("only_main".to_string(), "./skills/only-main.md".to_string());
        workflow.skills = Some(main_skills);
        workflow.include = Some(vec![IncludeSpec {
            path: Some("included.nika.yaml".to_string()),
            pkg: None,
            prefix: None,
        }]);

        // Expand includes
        let result = expand_includes(workflow, temp_dir.path()).unwrap();

        // ASSERT: Should have all skills, with main workflow taking precedence
        let skills = result.skills.expect("Skills should be present");
        assert_eq!(
            skills.len(),
            3,
            "Should have 3 unique skills (merged + deduplicated)"
        );

        // Main workflow skill should override included
        let shared_skill = skills.get("shared").expect("shared skill should exist");
        assert!(
            shared_skill.contains("main-shared"),
            "Main workflow skill should take precedence over included: got {}",
            shared_skill
        );

        // Both unique skills should be present
        assert!(
            skills.contains_key("only_main"),
            "Main-only skill should be present"
        );
        assert!(
            skills.contains_key("only_included"),
            "Included-only skill should be present"
        );
    }

    #[test]
    fn test_expand_includes_skills_from_multiple_includes() {
        let temp_dir = TempDir::new().unwrap();

        // Create first included workflow with skills
        let included1_yaml = r#"
schema: nika/workflow@0.9
provider: claude
skills:
  skill_a: "./skills/a.md"
  shared: "./skills/shared-from-first.md"
tasks:
  - id: task1
    infer: "Task 1"
"#;
        std::fs::write(temp_dir.path().join("first.nika.yaml"), included1_yaml).unwrap();

        // Create second included workflow with skills
        let included2_yaml = r#"
schema: nika/workflow@0.9
provider: claude
skills:
  skill_b: "./skills/b.md"
  shared: "./skills/shared-from-second.md"
tasks:
  - id: task2
    infer: "Task 2"
"#;
        std::fs::write(temp_dir.path().join("second.nika.yaml"), included2_yaml).unwrap();

        // Create main workflow with includes
        let mut workflow = make_test_workflow();
        workflow.include = Some(vec![
            IncludeSpec {
                path: Some("first.nika.yaml".to_string()),
                pkg: None,
                prefix: Some("first_".to_string()),
            },
            IncludeSpec {
                path: Some("second.nika.yaml".to_string()),
                pkg: None,
                prefix: Some("second_".to_string()),
            },
        ]);

        // Expand includes
        let result = expand_includes(workflow, temp_dir.path()).unwrap();

        // ASSERT: Should have skills from both includes
        let skills = result.skills.expect("Skills should be present");

        // Should have skill_a, skill_b, and one shared (first should win due to order)
        assert_eq!(
            skills.len(),
            3,
            "Should have 3 skills (skill_a + skill_b + one shared)"
        );
        assert!(skills.contains_key("skill_a"), "Should have skill_a");
        assert!(skills.contains_key("skill_b"), "Should have skill_b");

        // First include should win for shared skill (processed first)
        let shared = skills.get("shared").expect("shared skill should exist");
        assert!(
            shared.contains("shared-from-first"),
            "First included workflow's skill should take precedence: got {}",
            shared
        );
    }
}
