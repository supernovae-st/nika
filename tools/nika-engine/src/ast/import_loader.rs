// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Include Loader — Raw-level workflow include expansion
//!
//! Operates on the **raw AST** (before analysis), so the analyzer sees
//! all tasks (main + included) as one unified `RawWorkflow`.
//!
//! # Architecture Decision
//!
//! Include expansion at the RAW level means:
//! - The analyzer validates everything together (no cross-include issues)
//! - String-based prefix manipulation is simple at raw level
//! - No TaskId re-interning needed (analyzer creates all TaskIds from scratch)
//!
//! # Pipeline
//!
//! ```text
//! RawWorkflow (with include:)
//!   → expand_raw_include()
//!     → for each include: read file → raw::parse() → recurse → prefix → merge
//!   → RawWorkflow (flat, all tasks inlined)
//!     → analyze() → AnalyzedWorkflow
//! ```
//!
use std::collections::HashSet;
use std::path::Path;

use indexmap::IndexMap;

use super::raw::{parse, RawTask, RawWorkflow};
use crate::error::NikaError;
use crate::source::{FileId, Spanned};

/// Maximum depth for recursive includes (prevent infinite loops).
const MAX_INCLUDE_DEPTH: usize = 10;

/// Reserved binding namespaces that must NOT be prefixed.
///
/// These correspond to `BindingSource` variants that are not task references:
/// - `context` → `BindingSource::Context`
/// - `inputs`  → `BindingSource::Input`
/// - `env`     → `BindingSource::Env`
const RESERVED_NAMESPACES: &[&str] = &["context", "inputs", "env"];

/// Expand all includes in a raw workflow.
///
/// Recursively loads included workflow files and merges their tasks
/// into the main workflow. Task IDs and binding references are prefixed
/// according to each include spec's `prefix:` field.
///
/// # Arguments
///
/// * `workflow` - The raw workflow to expand
/// * `base_path` - Base directory for resolving relative include paths
///
/// # Returns
///
/// The workflow with all includes expanded and tasks inlined.
///
/// # Errors
///
/// Returns `NikaError` if:
/// - Include file not found or unreadable
/// - Include file has parse errors
/// - Circular include detected (same canonical path)
/// - Maximum include depth exceeded (> 10 levels)
/// - Path traversal detected (escaping base directory)
pub fn expand_raw_include(
    workflow: RawWorkflow,
    base_path: &Path,
) -> Result<RawWorkflow, NikaError> {
    expand_raw_include_recursive(workflow, base_path, 0, &mut HashSet::new())
}

/// Recursive include expansion with depth tracking and circular detection.
fn expand_raw_include_recursive(
    mut workflow: RawWorkflow,
    base_path: &Path,
    depth: usize,
    visited: &mut HashSet<String>,
) -> Result<RawWorkflow, NikaError> {
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
        Some(includes) if !includes.value.is_empty() => includes.value,
        _ => return Ok(workflow),
    };

    // Process each include
    for spanned_include in includes {
        let include_spec = &spanned_include.value;
        let include_path_str = &include_spec.path.value;

        // Resolve the filesystem path
        let resolved_path = base_path.join(include_path_str);

        // Security: Validate path stays within project boundary
        validate_path_boundary(base_path, &resolved_path)?;

        // Canonicalize for reliable circular detection
        let canonical_path =
            resolved_path
                .canonicalize()
                .map_err(|e| NikaError::WorkflowNotFound {
                    path: format!("{}: {}", resolved_path.display(), e),
                })?;
        let canonical_str = canonical_path.to_string_lossy().to_string();

        // Circular include detection
        if visited.contains(&canonical_str) {
            return Err(NikaError::ValidationError {
                reason: format!("Circular include detected: {}", include_path_str),
            });
        }
        visited.insert(canonical_str.clone());

        // Load and parse the included workflow
        let included_workflow = load_included_workflow(&canonical_path)?;

        // Recursively expand includes in the included workflow
        let include_base = canonical_path.parent().unwrap_or(Path::new("."));
        let expanded =
            expand_raw_include_recursive(included_workflow, include_base, depth + 1, visited)?;

        // Get prefix (if any)
        let prefix = include_spec.prefix.as_ref().map(|s| s.value.as_str());

        // Merge into main workflow
        merge_raw_workflow(&mut workflow, expanded, prefix)?;

        // Remove from visited after processing (allows same file in different branches)
        visited.remove(&canonical_str);
    }

    Ok(workflow)
}

/// Validate that a path stays within the project boundary.
fn validate_path_boundary(base_path: &Path, target_path: &Path) -> Result<(), NikaError> {
    nika_security::path::validate_canonicalized_boundary(base_path, target_path).map_err(|e| {
        if e.reason.contains("Cannot resolve target path") {
            NikaError::WorkflowNotFound {
                path: format!("{}: {}", e.target_path.display(), e.reason),
            }
        } else {
            NikaError::ValidationError { reason: e.reason }
        }
    })
}

/// Load and parse an included workflow file using the raw parser.
fn load_included_workflow(path: &Path) -> Result<RawWorkflow, NikaError> {
    let content = std::fs::read_to_string(path).map_err(|e| NikaError::WorkflowNotFound {
        path: format!("{}: {}", path.display(), e),
    })?;

    // Use a unique FileId for each included file.
    // We use a hash of the path to get a deterministic but unique ID.
    // The FileId is only used for span tracking within the raw AST.
    let file_id = FileId(path_hash(path));

    parse(&content, file_id).map_err(|e| NikaError::ParseError {
        details: format!(
            "Failed to parse included workflow '{}': {}",
            path.display(),
            e.message
        ),
    })
}

/// Simple hash of a path to generate a FileId.
fn path_hash(path: &Path) -> u32 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    // Avoid DUMMY FileId (u32::MAX)
    (hasher.finish() as u32) & 0x7FFF_FFFF
}

/// Merge an included (expanded) workflow into the main workflow.
///
/// - Tasks are prefixed and appended
/// - MCP servers are merged (main takes precedence on conflicts)
/// - Context files are NOT merged (top-level only, matching old behavior)
/// - Include field is already consumed (was `take()`n in recursive step)
fn merge_raw_workflow(
    main: &mut RawWorkflow,
    imported: RawWorkflow,
    prefix: Option<&str>,
) -> Result<(), NikaError> {
    // Collect existing task IDs for duplicate detection (owned to avoid borrow conflict)
    let existing_ids: std::collections::HashSet<String> = main
        .tasks
        .value
        .iter()
        .map(|t| t.value.id.value.clone())
        .collect();

    // Merge tasks with prefix
    for spanned_task in imported.tasks.value {
        let prefixed = prefix_raw_task(spanned_task, prefix);
        // Check for collision
        if existing_ids.contains(prefixed.value.id.value.as_str()) {
            let new_id = &prefixed.value.id.value;
            return Err(NikaError::ValidationError {
                reason: format!(
                    "Include conflict: task '{}' already exists in workflow. \
                     Use 'prefix:' to namespace included tasks.",
                    new_id
                ),
            });
        }

        main.tasks.value.push(prefixed);
    }

    // Merge MCP servers (main takes precedence on name conflicts)
    if let Some(imported_mcp) = imported.mcp {
        match main.mcp.as_mut() {
            Some(main_mcp) => {
                for (name, server) in imported_mcp.value.servers {
                    // Only insert if not already present in main
                    if !main_mcp.value.servers.contains_key(&name) {
                        main_mcp.value.servers.insert(name, server);
                    }
                }
            }
            None => {
                main.mcp = Some(imported_mcp);
            }
        }
    }

    Ok(())
}

/// Prefix a raw task's ID, with_refs values, and depends_on values.
fn prefix_raw_task(task: Spanned<RawTask>, prefix: Option<&str>) -> Spanned<RawTask> {
    let prefix = match prefix {
        Some(p) if !p.is_empty() => p,
        _ => return task,
    };

    let mut new_task = task.value.clone();

    // 1. Prefix the task ID
    new_task.id = Spanned::new(format!("{}{}", prefix, new_task.id.value), new_task.id.span);

    // 2. Prefix depends_on values
    if let Some(ref mut deps) = new_task.depends_on {
        deps.value = deps
            .value
            .iter()
            .map(|dep| Spanned::new(format!("{}{}", prefix, dep.value), dep.span))
            .collect();
    }

    // 3. Prefix with_refs values (binding expressions)
    if let Some(ref mut with_refs) = new_task.with_refs {
        let old_map = std::mem::take(&mut with_refs.value);
        let mut new_map = IndexMap::with_capacity(old_map.len());

        for (key, value) in old_map {
            let prefixed_value =
                Spanned::new(prefix_binding_expr(&value.value, prefix), value.span);
            new_map.insert(key, prefixed_value);
        }

        with_refs.value = new_map;
    }

    // 4. Prefix for_each items if it references a task
    if let Some(ref mut for_each) = new_task.for_each {
        let items = &for_each.value.items.value;
        let prefixed = prefix_binding_expr(items, prefix);
        if prefixed != *items {
            for_each.value.items = Spanned::new(prefixed, for_each.value.items.span);
        }
    }

    Spanned::new(new_task, task.span)
}

/// Prefix task references in a raw binding expression string.
///
/// Binding expressions follow this grammar:
/// ```text
/// entry     := path ("|" transform)* ("??" default)?
/// path      := identifier ("." identifier | "[" index "]")*
/// ```
///
/// We need to prefix the *leading identifier* if it's a task reference
/// (not a reserved namespace like `context`, `inputs`, `env`).
///
/// # Examples
///
/// ```text
/// "step1"                → "prefix_step1"
/// "step1.data"           → "prefix_step1.data"
/// "step1.data | upper"   → "prefix_step1.data | upper"
/// "step1.x ?? fallback"  → "prefix_step1.x ?? fallback"
/// "context.files.brand"  → "context.files.brand"    (reserved)
/// "inputs.name"          → "inputs.name"            (reserved)
/// "env.API_KEY"          → "env.API_KEY"            (reserved)
/// "$step1.data"          → "$prefix_step1.data"     (dollar prefix)
/// ""                     → ""                       (empty)
/// ```
fn prefix_binding_expr(expr: &str, prefix: &str) -> String {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return expr.to_string();
    }

    // Strip optional leading '$' (syntactic sugar)
    let (dollar, rest) = if let Some(stripped) = trimmed.strip_prefix('$') {
        ("$", stripped)
    } else {
        ("", trimmed)
    };

    if rest.is_empty() {
        return expr.to_string();
    }

    // Extract the leading identifier (before first '.', '[', '|', '?', or whitespace)
    let ident_end = rest
        .find(|c: char| c == '.' || c == '[' || c == '|' || c == '?' || c.is_whitespace())
        .unwrap_or(rest.len());

    let identifier = &rest[..ident_end];
    let remainder = &rest[ident_end..];

    // Don't prefix reserved namespaces or empty identifiers
    if identifier.is_empty() || is_reserved_namespace(identifier) {
        return expr.to_string();
    }

    // Prefix the task identifier
    format!("{}{}{}{}", dollar, prefix, identifier, remainder)
}

/// Check if an identifier is a reserved binding namespace.
fn is_reserved_namespace(ident: &str) -> bool {
    RESERVED_NAMESPACES.contains(&ident)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::raw::{RawIncludeSpec, RawMcpConfig, RawMcpServer};
    use crate::source::{FileId, Span};
    use tempfile::TempDir;

    fn make_span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), start, end)
    }

    // ─── prefix_binding_expr tests ───────────────────────────────

    #[test]
    fn test_prefix_binding_expr_simple_task_ref() {
        assert_eq!(prefix_binding_expr("step1", "setup_"), "setup_step1");
    }

    #[test]
    fn test_prefix_binding_expr_dotted_path() {
        assert_eq!(
            prefix_binding_expr("step1.data", "setup_"),
            "setup_step1.data"
        );
    }

    #[test]
    fn test_prefix_binding_expr_deep_path() {
        assert_eq!(
            prefix_binding_expr("step1.data.items[0]", "setup_"),
            "setup_step1.data.items[0]"
        );
    }

    #[test]
    fn test_prefix_binding_expr_with_transform() {
        assert_eq!(
            prefix_binding_expr("step1.data | upper", "setup_"),
            "setup_step1.data | upper"
        );
    }

    #[test]
    fn test_prefix_binding_expr_with_default() {
        assert_eq!(
            prefix_binding_expr("step1.x ?? fallback", "setup_"),
            "setup_step1.x ?? fallback"
        );
    }

    #[test]
    fn test_prefix_binding_expr_with_transform_and_default() {
        assert_eq!(
            prefix_binding_expr("step1.data | upper | trim ?? \"none\"", "pre_"),
            "pre_step1.data | upper | trim ?? \"none\""
        );
    }

    #[test]
    fn test_prefix_binding_expr_reserved_context() {
        assert_eq!(
            prefix_binding_expr("context.files.brand", "setup_"),
            "context.files.brand"
        );
    }

    #[test]
    fn test_prefix_binding_expr_reserved_inputs() {
        assert_eq!(prefix_binding_expr("inputs.name", "setup_"), "inputs.name");
    }

    #[test]
    fn test_prefix_binding_expr_reserved_env() {
        assert_eq!(prefix_binding_expr("env.API_KEY", "setup_"), "env.API_KEY");
    }

    #[test]
    fn test_prefix_binding_expr_dollar_prefix() {
        assert_eq!(
            prefix_binding_expr("$step1.data", "setup_"),
            "$setup_step1.data"
        );
    }

    #[test]
    fn test_prefix_binding_expr_dollar_simple() {
        assert_eq!(prefix_binding_expr("$step1", "pre_"), "$pre_step1");
    }

    #[test]
    fn test_prefix_binding_expr_empty() {
        assert_eq!(prefix_binding_expr("", "setup_"), "");
    }

    #[test]
    fn test_prefix_binding_expr_whitespace() {
        assert_eq!(prefix_binding_expr("  ", "setup_"), "  ");
    }

    #[test]
    fn test_prefix_binding_expr_dollar_only() {
        assert_eq!(prefix_binding_expr("$", "setup_"), "$");
    }

    #[test]
    fn test_prefix_binding_expr_reserved_with_dollar() {
        // $context should NOT be prefixed (context is reserved)
        assert_eq!(
            prefix_binding_expr("$context.files", "setup_"),
            "$context.files"
        );
    }

    // ─── is_reserved_namespace tests ─────────────────────────────

    #[test]
    fn test_reserved_namespaces() {
        assert!(is_reserved_namespace("context"));
        assert!(is_reserved_namespace("inputs"));
        assert!(is_reserved_namespace("env"));
        assert!(!is_reserved_namespace("step1"));
        assert!(!is_reserved_namespace("my_task"));
        assert!(!is_reserved_namespace(""));
    }

    // ─── prefix_raw_task tests ───────────────────────────────────

    #[test]
    fn test_prefix_raw_task_no_prefix() {
        let task = Spanned::new(RawTask::new("step1"), make_span(0, 10));
        let result = prefix_raw_task(task, None);
        assert_eq!(result.value.id.value, "step1");
    }

    #[test]
    fn test_prefix_raw_task_empty_prefix() {
        let task = Spanned::new(RawTask::new("step1"), make_span(0, 10));
        let result = prefix_raw_task(task, Some(""));
        assert_eq!(result.value.id.value, "step1");
    }

    #[test]
    fn test_prefix_raw_task_id() {
        let task = Spanned::new(RawTask::new("step1"), make_span(0, 10));
        let result = prefix_raw_task(task, Some("setup_"));
        assert_eq!(result.value.id.value, "setup_step1");
    }

    #[test]
    fn test_prefix_raw_task_depends_on() {
        let mut task = RawTask::new("consumer");
        task.depends_on = Some(Spanned::dummy(vec![
            Spanned::dummy("step1".to_string()),
            Spanned::dummy("step2".to_string()),
        ]));

        let spanned = Spanned::new(task, make_span(0, 50));
        let result = prefix_raw_task(spanned, Some("lib_"));

        assert_eq!(result.value.id.value, "lib_consumer");
        let deps: Vec<&str> = result
            .value
            .depends_on
            .as_ref()
            .unwrap()
            .value
            .iter()
            .map(|s| s.value.as_str())
            .collect();
        assert_eq!(deps, vec!["lib_step1", "lib_step2"]);
    }

    #[test]
    fn test_prefix_raw_task_with_refs() {
        let mut task = RawTask::new("consumer");
        let mut with_refs = IndexMap::new();
        with_refs.insert(
            Spanned::dummy("data".to_string()),
            Spanned::dummy("producer.output".to_string()),
        );
        with_refs.insert(
            Spanned::dummy("cfg".to_string()),
            Spanned::dummy("env.API_KEY".to_string()),
        );
        with_refs.insert(
            Spanned::dummy("ctx".to_string()),
            Spanned::dummy("context.files.brand".to_string()),
        );
        task.with_refs = Some(Spanned::dummy(with_refs));

        let spanned = Spanned::new(task, make_span(0, 100));
        let result = prefix_raw_task(spanned, Some("lib_"));

        let refs = &result.value.with_refs.as_ref().unwrap().value;
        // Task ref should be prefixed
        assert_eq!(refs.get_index(0).unwrap().1.value, "lib_producer.output");
        // env should NOT be prefixed
        assert_eq!(refs.get_index(1).unwrap().1.value, "env.API_KEY");
        // context should NOT be prefixed
        assert_eq!(refs.get_index(2).unwrap().1.value, "context.files.brand");
    }

    #[test]
    fn test_prefix_raw_task_with_refs_transforms() {
        let mut task = RawTask::new("consumer");
        let mut with_refs = IndexMap::new();
        with_refs.insert(
            Spanned::dummy("val".to_string()),
            Spanned::dummy("step1.data | upper ?? \"none\"".to_string()),
        );
        task.with_refs = Some(Spanned::dummy(with_refs));

        let spanned = Spanned::new(task, make_span(0, 80));
        let result = prefix_raw_task(spanned, Some("p_"));

        let refs = &result.value.with_refs.as_ref().unwrap().value;
        assert_eq!(
            refs.get_index(0).unwrap().1.value,
            "p_step1.data | upper ?? \"none\""
        );
    }

    // ─── merge_raw_workflow tests ────────────────────────────────

    #[test]
    fn test_merge_workflow_tasks() {
        let mut main = RawWorkflow {
            tasks: Spanned::dummy(vec![Spanned::dummy(RawTask::new("main_task"))]),
            ..Default::default()
        };

        let imported = RawWorkflow {
            tasks: Spanned::dummy(vec![
                Spanned::dummy(RawTask::new("step1")),
                Spanned::dummy(RawTask::new("step2")),
            ]),
            ..Default::default()
        };

        merge_raw_workflow(&mut main, imported, Some("lib_")).unwrap();

        assert_eq!(main.tasks.value.len(), 3);
        assert_eq!(main.tasks.value[0].value.id.value, "main_task");
        assert_eq!(main.tasks.value[1].value.id.value, "lib_step1");
        assert_eq!(main.tasks.value[2].value.id.value, "lib_step2");
    }

    #[test]
    fn test_merge_workflow_no_prefix() {
        let mut main = RawWorkflow {
            tasks: Spanned::dummy(vec![]),
            ..Default::default()
        };

        let imported = RawWorkflow {
            tasks: Spanned::dummy(vec![Spanned::dummy(RawTask::new("task_a"))]),
            ..Default::default()
        };

        merge_raw_workflow(&mut main, imported, None).unwrap();

        assert_eq!(main.tasks.value.len(), 1);
        assert_eq!(main.tasks.value[0].value.id.value, "task_a");
    }

    #[test]
    fn test_merge_workflow_mcp_servers() {
        let mut main_mcp = RawMcpConfig::new();
        main_mcp.servers.insert(
            Spanned::dummy("novanet".to_string()),
            Spanned::dummy(RawMcpServer::with_command("cargo run")),
        );
        let mut main = RawWorkflow {
            mcp: Some(Spanned::dummy(main_mcp)),
            tasks: Spanned::dummy(vec![]),
            ..Default::default()
        };

        let mut imported_mcp = RawMcpConfig::new();
        // Same name as main (should NOT overwrite)
        imported_mcp.servers.insert(
            Spanned::dummy("novanet".to_string()),
            Spanned::dummy(RawMcpServer::with_command("other command")),
        );
        // New name (should be added)
        imported_mcp.servers.insert(
            Spanned::dummy("perplexity".to_string()),
            Spanned::dummy(RawMcpServer::with_command("npx perplexity")),
        );
        let imported = RawWorkflow {
            mcp: Some(Spanned::dummy(imported_mcp)),
            tasks: Spanned::dummy(vec![]),
            ..Default::default()
        };

        merge_raw_workflow(&mut main, imported, None).unwrap();

        let mcp = main.mcp.as_ref().unwrap();
        assert_eq!(mcp.value.server_count(), 2);
        assert!(mcp.value.has_server("novanet"));
        assert!(mcp.value.has_server("perplexity"));
        // novanet should keep main's command
        let novanet = mcp.value.get_server("novanet").unwrap();
        assert_eq!(novanet.value.command.as_ref().unwrap().value, "cargo run");
    }

    #[test]
    fn test_merge_workflow_mcp_servers_main_has_none() {
        // main has no MCP config
        let mut main = RawWorkflow {
            tasks: Spanned::dummy(vec![]),
            ..Default::default()
        };

        let mut imported_mcp = RawMcpConfig::new();
        imported_mcp.servers.insert(
            Spanned::dummy("perplexity".to_string()),
            Spanned::dummy(RawMcpServer::with_command("npx perplexity")),
        );
        let imported = RawWorkflow {
            mcp: Some(Spanned::dummy(imported_mcp)),
            tasks: Spanned::dummy(vec![]),
            ..Default::default()
        };

        merge_raw_workflow(&mut main, imported, None).unwrap();

        assert!(main.mcp.is_some());
        assert_eq!(main.mcp.as_ref().unwrap().value.server_count(), 1);
    }

    // ─── File-based integration tests (tempfile) ─────────────────

    #[test]
    fn test_expand_include_no_imports() {
        let workflow = RawWorkflow {
            schema: Spanned::dummy("nika/workflow@0.12".to_string()),
            tasks: Spanned::dummy(vec![Spanned::dummy(RawTask::new("step1"))]),
            ..Default::default()
        };

        let result = expand_raw_include(workflow, Path::new(".")).unwrap();
        assert_eq!(result.tasks.value.len(), 1);
        assert_eq!(result.tasks.value[0].value.id.value, "step1");
    }

    #[test]
    fn test_expand_include_simple() {
        let dir = TempDir::new().unwrap();

        // Create included workflow
        let included_yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: init
    infer: "Initialize"
  - id: setup
    depends_on: [init]
    infer: "Setup"
"#;
        let include_path = dir.path().join("setup.nika.yaml");
        std::fs::write(&include_path, included_yaml).unwrap();

        // Create main workflow with include
        let main = RawWorkflow {
            schema: Spanned::dummy("nika/workflow@0.12".to_string()),
            include: Some(Spanned::dummy(vec![Spanned::dummy(RawIncludeSpec {
                path: Spanned::dummy("setup.nika.yaml".to_string()),
                prefix: Some(Spanned::dummy("setup_".to_string())),
                span: Span::dummy(),
            })])),
            tasks: Spanned::dummy(vec![Spanned::dummy(RawTask::new("main_task"))]),
            ..Default::default()
        };

        let result = expand_raw_include(main, dir.path()).unwrap();

        // Should have 3 tasks: main_task + setup_init + setup_setup
        assert_eq!(result.tasks.value.len(), 3);
        let ids: Vec<&str> = result
            .tasks
            .value
            .iter()
            .map(|t| t.value.id.value.as_str())
            .collect();
        assert!(ids.contains(&"main_task"));
        assert!(ids.contains(&"setup_init"));
        assert!(ids.contains(&"setup_setup"));

        // setup_setup should have prefixed depends_on
        let setup_setup = result
            .tasks
            .value
            .iter()
            .find(|t| t.value.id.value == "setup_setup")
            .unwrap();
        let deps: Vec<&str> = setup_setup
            .value
            .depends_on
            .as_ref()
            .unwrap()
            .value
            .iter()
            .map(|s| s.value.as_str())
            .collect();
        assert_eq!(deps, vec!["setup_init"]);
    }

    #[test]
    fn test_expand_include_no_prefix() {
        let dir = TempDir::new().unwrap();

        let included_yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: helper
    infer: "Help"
"#;
        std::fs::write(dir.path().join("helper.nika.yaml"), included_yaml).unwrap();

        let main = RawWorkflow {
            schema: Spanned::dummy("nika/workflow@0.12".to_string()),
            include: Some(Spanned::dummy(vec![Spanned::dummy(RawIncludeSpec {
                path: Spanned::dummy("helper.nika.yaml".to_string()),
                prefix: None,
                span: Span::dummy(),
            })])),
            tasks: Spanned::dummy(vec![]),
            ..Default::default()
        };

        let result = expand_raw_include(main, dir.path()).unwrap();
        assert_eq!(result.tasks.value.len(), 1);
        assert_eq!(result.tasks.value[0].value.id.value, "helper");
    }

    #[test]
    fn test_expand_include_recursive() {
        let dir = TempDir::new().unwrap();

        // Level 2: deepest include
        let deep_yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: deep_task
    infer: "Deep"
"#;
        std::fs::write(dir.path().join("deep.nika.yaml"), deep_yaml).unwrap();

        // Level 1: mid-level include that includes deep
        let mid_yaml = r#"
schema: "nika/workflow@0.12"
include:
  - path: deep.nika.yaml
    prefix: deep_
tasks:
  - id: mid_task
    infer: "Mid"
"#;
        std::fs::write(dir.path().join("mid.nika.yaml"), mid_yaml).unwrap();

        // Level 0: main workflow
        let main = RawWorkflow {
            schema: Spanned::dummy("nika/workflow@0.12".to_string()),
            include: Some(Spanned::dummy(vec![Spanned::dummy(RawIncludeSpec {
                path: Spanned::dummy("mid.nika.yaml".to_string()),
                prefix: Some(Spanned::dummy("m_".to_string())),
                span: Span::dummy(),
            })])),
            tasks: Spanned::dummy(vec![Spanned::dummy(RawTask::new("root"))]),
            ..Default::default()
        };

        let result = expand_raw_include(main, dir.path()).unwrap();

        let ids: Vec<&str> = result
            .tasks
            .value
            .iter()
            .map(|t| t.value.id.value.as_str())
            .collect();

        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"root"));
        assert!(ids.contains(&"m_mid_task"));
        // deep_task gets prefixed by mid (deep_) then by main (m_)
        assert!(ids.contains(&"m_deep_deep_task"));
    }

    #[test]
    fn test_expand_include_circular_detection() {
        let dir = TempDir::new().unwrap();

        // A includes B
        let a_yaml = r#"
schema: "nika/workflow@0.12"
include:
  - path: b.nika.yaml
tasks:
  - id: a_task
    infer: "A"
"#;
        std::fs::write(dir.path().join("a.nika.yaml"), a_yaml).unwrap();

        // B includes A (circular!)
        let b_yaml = r#"
schema: "nika/workflow@0.12"
include:
  - path: a.nika.yaml
tasks:
  - id: b_task
    infer: "B"
"#;
        std::fs::write(dir.path().join("b.nika.yaml"), b_yaml).unwrap();

        // Start from A
        let a_content = std::fs::read_to_string(dir.path().join("a.nika.yaml")).unwrap();
        let a_workflow = parse(&a_content, FileId(0)).unwrap();

        let result = expand_raw_include(a_workflow, dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Circular include"),
            "Expected circular include error, got: {}",
            msg
        );
    }

    #[test]
    fn test_expand_include_depth_limit() {
        let dir = TempDir::new().unwrap();

        // Create a chain of 12 includes (exceeds MAX_INCLUDE_DEPTH = 10)
        for i in 0..12 {
            let next = i + 1;
            let yaml = if next <= 11 {
                format!(
                    r#"
schema: "nika/workflow@0.12"
include:
  - path: level{}.nika.yaml
tasks:
  - id: task{}
    infer: "Level {}"
"#,
                    next, i, i
                )
            } else {
                format!(
                    r#"
schema: "nika/workflow@0.12"
tasks:
  - id: task{}
    infer: "Level {}"
"#,
                    i, i
                )
            };
            std::fs::write(dir.path().join(format!("level{}.nika.yaml", i)), yaml).unwrap();
        }

        let content = std::fs::read_to_string(dir.path().join("level0.nika.yaml")).unwrap();
        let workflow = parse(&content, FileId(0)).unwrap();

        let result = expand_raw_include(workflow, dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Maximum include depth"),
            "Expected depth limit error, got: {}",
            msg
        );
    }

    #[test]
    fn test_expand_include_file_not_found() {
        let dir = TempDir::new().unwrap();

        let main = RawWorkflow {
            schema: Spanned::dummy("nika/workflow@0.12".to_string()),
            include: Some(Spanned::dummy(vec![Spanned::dummy(RawIncludeSpec {
                path: Spanned::dummy("nonexistent.nika.yaml".to_string()),
                prefix: None,
                span: Span::dummy(),
            })])),
            tasks: Spanned::dummy(vec![]),
            ..Default::default()
        };

        let result = expand_raw_include(main, dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_include_path_traversal() {
        let dir = TempDir::new().unwrap();

        // Attempt to escape the base directory
        let main = RawWorkflow {
            schema: Spanned::dummy("nika/workflow@0.12".to_string()),
            include: Some(Spanned::dummy(vec![Spanned::dummy(RawIncludeSpec {
                path: Spanned::dummy("../../../etc/passwd".to_string()),
                prefix: None,
                span: Span::dummy(),
            })])),
            tasks: Spanned::dummy(vec![]),
            ..Default::default()
        };

        let result = expand_raw_include(main, dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_include_with_refs_prefixing() {
        let dir = TempDir::new().unwrap();

        let included_yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: producer
    infer: "Produce data"
  - id: consumer
    with:
      data: producer.output
      cfg: env.API_KEY
      ctx: context.files.brand
    depends_on: [producer]
    infer: "Consume: {{with.data}}"
"#;
        std::fs::write(dir.path().join("lib.nika.yaml"), included_yaml).unwrap();

        let main = RawWorkflow {
            schema: Spanned::dummy("nika/workflow@0.12".to_string()),
            include: Some(Spanned::dummy(vec![Spanned::dummy(RawIncludeSpec {
                path: Spanned::dummy("lib.nika.yaml".to_string()),
                prefix: Some(Spanned::dummy("lib_".to_string())),
                span: Span::dummy(),
            })])),
            tasks: Spanned::dummy(vec![Spanned::dummy(RawTask::new("main_step"))]),
            ..Default::default()
        };

        let result = expand_raw_include(main, dir.path()).unwrap();

        // Find lib_consumer
        let consumer = result
            .tasks
            .value
            .iter()
            .find(|t| t.value.id.value == "lib_consumer")
            .expect("lib_consumer should exist");

        let refs = consumer.value.with_refs.as_ref().unwrap();

        // Task ref should be prefixed
        let data_val = refs
            .value
            .iter()
            .find(|(k, _)| k.value == "data")
            .unwrap()
            .1;
        assert_eq!(data_val.value, "lib_producer.output");

        // env should NOT be prefixed
        let cfg_val = refs.value.iter().find(|(k, _)| k.value == "cfg").unwrap().1;
        assert_eq!(cfg_val.value, "env.API_KEY");

        // context should NOT be prefixed
        let ctx_val = refs.value.iter().find(|(k, _)| k.value == "ctx").unwrap().1;
        assert_eq!(ctx_val.value, "context.files.brand");

        // depends_on should be prefixed
        let deps: Vec<&str> = consumer
            .value
            .depends_on
            .as_ref()
            .unwrap()
            .value
            .iter()
            .map(|s| s.value.as_str())
            .collect();
        assert_eq!(deps, vec!["lib_producer"]);
    }

    #[test]
    fn test_expand_include_mcp_merge() {
        let dir = TempDir::new().unwrap();

        let included_yaml = r#"
schema: "nika/workflow@0.12"
mcp:
  servers:
    perplexity:
      command: npx
      args: ["-y", "perplexity-mcp"]
tasks:
  - id: search
    infer: "Search"
"#;
        std::fs::write(dir.path().join("search.nika.yaml"), included_yaml).unwrap();

        let mut main_mcp = RawMcpConfig::new();
        main_mcp.servers.insert(
            Spanned::dummy("novanet".to_string()),
            Spanned::dummy(RawMcpServer::with_command("cargo run")),
        );

        let main = RawWorkflow {
            schema: Spanned::dummy("nika/workflow@0.12".to_string()),
            mcp: Some(Spanned::dummy(main_mcp)),
            include: Some(Spanned::dummy(vec![Spanned::dummy(RawIncludeSpec {
                path: Spanned::dummy("search.nika.yaml".to_string()),
                prefix: Some(Spanned::dummy("s_".to_string())),
                span: Span::dummy(),
            })])),
            tasks: Spanned::dummy(vec![]),
            ..Default::default()
        };

        let result = expand_raw_include(main, dir.path()).unwrap();

        let mcp = result.mcp.as_ref().unwrap();
        assert_eq!(mcp.value.server_count(), 2);
        assert!(mcp.value.has_server("novanet"));
        assert!(mcp.value.has_server("perplexity"));
    }

    #[test]
    fn test_expand_include_same_file_different_branches() {
        let dir = TempDir::new().unwrap();

        // Shared utility
        let util_yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: util
    infer: "Utility"
"#;
        std::fs::write(dir.path().join("util.nika.yaml"), util_yaml).unwrap();

        // Branch A includes util
        let a_yaml = r#"
schema: "nika/workflow@0.12"
include:
  - path: util.nika.yaml
    prefix: u_
tasks:
  - id: a_task
    infer: "A"
"#;
        std::fs::write(dir.path().join("a.nika.yaml"), a_yaml).unwrap();

        // Branch B also includes util (should work — same file in different branches)
        let b_yaml = r#"
schema: "nika/workflow@0.12"
include:
  - path: util.nika.yaml
    prefix: v_
tasks:
  - id: b_task
    infer: "B"
"#;
        std::fs::write(dir.path().join("b.nika.yaml"), b_yaml).unwrap();

        // Main includes both A and B
        let main = RawWorkflow {
            schema: Spanned::dummy("nika/workflow@0.12".to_string()),
            include: Some(Spanned::dummy(vec![
                Spanned::dummy(RawIncludeSpec {
                    path: Spanned::dummy("a.nika.yaml".to_string()),
                    prefix: Some(Spanned::dummy("a_".to_string())),
                    span: Span::dummy(),
                }),
                Spanned::dummy(RawIncludeSpec {
                    path: Spanned::dummy("b.nika.yaml".to_string()),
                    prefix: Some(Spanned::dummy("b_".to_string())),
                    span: Span::dummy(),
                }),
            ])),
            tasks: Spanned::dummy(vec![Spanned::dummy(RawTask::new("root"))]),
            ..Default::default()
        };

        let result = expand_raw_include(main, dir.path()).unwrap();

        let ids: Vec<&str> = result
            .tasks
            .value
            .iter()
            .map(|t| t.value.id.value.as_str())
            .collect();

        // Should have: root, a_a_task, a_u_util, b_b_task, b_v_util
        assert!(ids.contains(&"root"));
        assert!(ids.contains(&"a_a_task"));
        assert!(ids.contains(&"a_u_util"));
        assert!(ids.contains(&"b_b_task"));
        assert!(ids.contains(&"b_v_util"));
    }

    // ─── merge_raw_workflow duplicate detection ─────────────────

    #[test]
    fn merge_rejects_duplicate_task_id_without_prefix() {
        let mut main = RawWorkflow {
            tasks: Spanned::dummy(vec![Spanned::dummy(RawTask::new("init"))]),
            ..Default::default()
        };

        let imported = RawWorkflow {
            tasks: Spanned::dummy(vec![Spanned::dummy(RawTask::new("init"))]),
            ..Default::default()
        };

        let result = merge_raw_workflow(&mut main, imported, None);
        assert!(result.is_err(), "should reject duplicate task ID 'init'");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("init"),
            "error should name the duplicate: {err}"
        );
    }

    #[test]
    fn merge_allows_duplicate_task_id_with_prefix() {
        let mut main = RawWorkflow {
            tasks: Spanned::dummy(vec![Spanned::dummy(RawTask::new("init"))]),
            ..Default::default()
        };

        let imported = RawWorkflow {
            tasks: Spanned::dummy(vec![Spanned::dummy(RawTask::new("init"))]),
            ..Default::default()
        };

        // With prefix "setup_", the ID becomes "setup_init" → no conflict
        let result = merge_raw_workflow(&mut main, imported, Some("setup_"));
        assert!(result.is_ok());
    }
}
