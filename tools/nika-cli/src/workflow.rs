//! Workflow management subcommand handler

use clap::Subcommand;
use std::path::PathBuf;

use colored::Colorize;

use nika_engine::ast::{parse_analyzed, parse_workflow};
use nika_engine::dag::Dag;
use nika_engine::error::NikaError;

/// Workflow management actions
#[derive(Subcommand)]
pub enum WorkflowAction {
    /// Open workflow in interactive editor
    Edit {
        /// Path to .nika.yaml file
        file: PathBuf,
    },

    /// Add a new task interactively
    AddTask {
        /// Path to .nika.yaml file
        file: PathBuf,

        /// Task ID (generated if not provided)
        #[arg(long)]
        id: Option<String>,

        /// Task verb (infer, exec, fetch, invoke, agent)
        #[arg(long, value_name = "VERB")]
        verb: Option<String>,

        /// Insert after this task ID
        #[arg(long)]
        after: Option<String>,
    },

    /// Visualize workflow as DAG graph
    Graph {
        /// Path to .nika.yaml file
        file: PathBuf,

        /// Output format: ascii, dot, mermaid
        #[arg(short, long, default_value = "ascii")]
        format: String,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Validate workflow with suggestions for improvements
    Check {
        /// Path to .nika.yaml file
        file: PathBuf,

        /// Show improvement suggestions
        #[arg(long)]
        suggest: bool,

        /// Output format: text, json
        #[arg(long, default_value = "text")]
        format: String,
    },
}

pub async fn handle_workflow_command(action: WorkflowAction, quiet: bool) -> Result<(), NikaError> {
    match action {
        WorkflowAction::Edit { file } => {
            // TUI Studio editor lives in the nika binary crate, not nika-cli.
            // When called from `nika`, main.rs overrides this via the tui module.
            let _ = (file, quiet);
            Err(NikaError::ConfigError {
                reason: "TUI feature not enabled. Rebuild with `--features tui`".to_string(),
            })
        }

        WorkflowAction::AddTask {
            file,
            id,
            verb,
            after,
        } => {
            // Validate file exists
            if !file.exists() {
                return Err(NikaError::WorkflowNotFound {
                    path: file.to_string_lossy().to_string(),
                });
            }

            // Read existing workflow
            let content = std::fs::read_to_string(&file)?;

            // Generate task ID if not provided
            let task_id = id.unwrap_or_else(|| {
                format!("task_{}", chrono::Utc::now().timestamp_millis() % 10000)
            });

            // Default verb is infer
            let task_verb = verb.unwrap_or_else(|| "infer".to_string());

            // Build the new task YAML
            let new_task = match task_verb.as_str() {
                "infer" => format!(
                    r#"  - id: {task_id}
    infer: "TODO: Add your prompt here"
"#
                ),
                "exec" => format!(
                    r#"  - id: {task_id}
    exec: "echo 'TODO: Add your command here'"
"#
                ),
                "fetch" => format!(
                    r#"  - id: {task_id}
    fetch:
      url: "https://example.com/api"
      method: GET
"#
                ),
                "invoke" => format!(
                    r#"  - id: {task_id}
    invoke:
      mcp: novanet
      tool: novanet_context
      params: {{}}
"#
                ),
                "agent" => format!(
                    r#"  - id: {task_id}
    agent:
      prompt: "TODO: Add your agent prompt here"
      max_turns: 5
"#
                ),
                _ => {
                    return Err(NikaError::ValidationError {
                        reason: format!(
                            "Unknown verb '{task_verb}'. Valid: infer, exec, fetch, invoke, agent"
                        ),
                    });
                }
            };

            // Find insertion point
            let mut lines: Vec<&str> = content.lines().collect();
            let mut insert_index = None;

            // Find tasks: section and optionally the task to insert after
            let mut in_tasks = false;
            let mut after_task_end = None;

            for (i, line) in lines.iter().enumerate() {
                if line.trim() == "tasks:" {
                    in_tasks = true;
                    continue;
                }
                if in_tasks {
                    // Check if this is a task start (- id:)
                    if line.trim().starts_with("- id:") {
                        // Check if this is the task we want to insert after
                        if let Some(ref after_id) = after {
                            if line.contains(after_id) {
                                // Mark that we found the after task
                                after_task_end = Some(i);
                            } else if after_task_end.is_some() {
                                // We've found the next task after our target
                                insert_index = Some(i);
                                break;
                            }
                        }
                    }
                    // Check for top-level sections (context:, mcp:, etc.)
                    if !line.starts_with(' ') && !line.starts_with('-') && line.contains(':') {
                        insert_index = Some(i);
                        break;
                    }
                }
            }

            // If no insert point found, append at end of tasks
            let insert_at = insert_index.unwrap_or(lines.len());

            // Insert the new task
            let new_task_lines: Vec<&str> = new_task.lines().collect();
            for (j, task_line) in new_task_lines.iter().enumerate() {
                lines.insert(insert_at + j, task_line);
            }

            // Write back
            let new_content = lines.join("\n");
            std::fs::write(&file, new_content)?;

            if !quiet {
                println!(
                    "{} Added task '{}' ({}) to {}",
                    "✓".green(),
                    task_id.cyan(),
                    task_verb.yellow(),
                    file.display()
                );
                if let Some(after_id) = after {
                    println!("  {} Inserted after task '{}'", "→".cyan(), after_id);
                }
            }

            Ok(())
        }

        WorkflowAction::Graph {
            file,
            format,
            output,
        } => {
            // Validate file exists
            if !file.exists() {
                return Err(NikaError::WorkflowNotFound {
                    path: file.to_string_lossy().to_string(),
                });
            }

            // Parse workflow and build DAG (includes both depends_on and with: edges)
            let content = std::fs::read_to_string(&file)?;
            let workflow = parse_workflow(&content)?;
            let dag = Dag::from_workflow(&workflow)?;
            let edges = dag.edges();

            // Generate graph based on format
            let graph_output = match format.as_str() {
                "ascii" => generate_ascii_dag(&workflow, &edges),
                "dot" => generate_dot_dag(&workflow, &edges),
                "mermaid" => generate_mermaid_dag(&workflow, &edges),
                _ => {
                    return Err(NikaError::ValidationError {
                        reason: format!("Unknown format '{format}'. Valid: ascii, dot, mermaid"),
                    });
                }
            };

            // Output
            match output {
                Some(path) => {
                    std::fs::write(&path, &graph_output)?;
                    if !quiet {
                        println!(
                            "{} DAG written to {} (format: {})",
                            "✓".green(),
                            path.display(),
                            format.cyan()
                        );
                    }
                }
                None => {
                    println!("{graph_output}");
                }
            }

            Ok(())
        }

        WorkflowAction::Check {
            file,
            suggest,
            format,
        } => {
            // Validate file exists
            if !file.exists() {
                if format == "json" {
                    let error_json = serde_json::json!({
                        "valid": false,
                        "file": file.to_string_lossy(),
                        "error": format!("Workflow not found: {}", file.display()),
                    });
                    println!("{}", serde_json::to_string_pretty(&error_json)?);
                }
                return Err(NikaError::WorkflowNotFound {
                    path: file.to_string_lossy().to_string(),
                });
            }

            // Parse and validate — catch parse errors for JSON output
            let content = std::fs::read_to_string(&file)?;
            let workflow = match parse_workflow(&content) {
                Ok(w) => w,
                Err(e) => {
                    if format == "json" {
                        let error_json = serde_json::json!({
                            "valid": false,
                            "file": file.to_string_lossy(),
                            "error": e.to_string(),
                        });
                        println!("{}", serde_json::to_string_pretty(&error_json)?);
                    }
                    return Err(e);
                }
            };

            // Collect validation results
            let mut issues: Vec<(String, String, String)> = Vec::new(); // (level, code, message)
            let mut suggestions: Vec<String> = Vec::new();

            // Check schema version
            let schema = workflow.schema.clone();
            if !schema.starts_with("nika/workflow@") {
                issues.push((
                    "error".to_string(),
                    "NIKA-001".to_string(),
                    "Missing or invalid schema version".to_string(),
                ));
            } else if let Some(version) = schema.strip_prefix("nika/workflow@") {
                if version != "0.12" && suggest {
                    suggestions.push(format!(
                        "Consider upgrading from @{version} to @0.12 for latest features"
                    ));
                }
            }

            // Check for common issues
            if workflow.tasks.is_empty() {
                issues.push((
                    "error".to_string(),
                    "NIKA-010".to_string(),
                    "Workflow has no tasks".to_string(),
                ));
            }

            // Check for duplicate task IDs
            let mut seen_ids = std::collections::HashSet::new();
            for task in &workflow.tasks {
                if !seen_ids.insert(&task.id) {
                    issues.push((
                        "error".to_string(),
                        "NIKA-141".to_string(),
                        format!("Duplicate task ID: '{}'", task.id),
                    ));
                }
            }

            // Check provider API keys (BUG 6 / NIKA-032)
            // Parse the analyzed workflow to access per-task providers
            {
                let mut providers_used: std::collections::HashSet<String> =
                    std::collections::HashSet::new();

                // Workflow default provider
                providers_used.insert(workflow.provider.to_string());

                // Per-task providers from analyzed AST
                if let Ok(analyzed) = parse_analyzed(&content) {
                    for task in &analyzed.tasks {
                        if let Some(ref p) = task.provider {
                            providers_used.insert(p.to_string());
                        }
                    }
                }

                // Check each provider for its env var
                for provider_name in &providers_used {
                    if let Some(provider) = nika_engine::core::find_provider(provider_name) {
                        if provider.requires_key && !provider.has_env_key() {
                            issues.push((
                                "warn".to_string(),
                                "NIKA-032".to_string(),
                                format!(
                                    "{} not set (provider '{}' used in workflow)",
                                    provider.env_var, provider_name
                                ),
                            ));
                        }
                    }
                }
            }

            // Validate file paths: skills and context files must exist on disk
            {
                let base_dir = file
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."));

                // Skills file paths
                if let Some(ref skills) = workflow.skills {
                    for (alias, path) in skills {
                        // Skip pkg: URIs (resolved at runtime from registry)
                        if path.starts_with("pkg:") {
                            continue;
                        }
                        // Skip template paths (contain {{...}})
                        if path.contains("{{") {
                            continue;
                        }
                        let resolved = base_dir.join(path);
                        if !resolved.exists() {
                            issues.push((
                                "error".to_string(),
                                "NIKA-270".to_string(),
                                format!(
                                    "Skill '{}' file not found: {} (resolved: {})",
                                    alias,
                                    path,
                                    resolved.display()
                                ),
                            ));
                        }
                    }
                }

                // Context file paths
                if let Some(ref ctx) = workflow.context {
                    for (alias, path) in &ctx.files {
                        // Skip glob patterns and template paths
                        if path.contains('*')
                            || path.contains('?')
                            || path.contains("{{")
                        {
                            continue;
                        }
                        let resolved = base_dir.join(path);
                        if !resolved.exists() {
                            issues.push((
                                "error".to_string(),
                                "NIKA-250".to_string(),
                                format!(
                                    "Context file '{}' not found: {} (resolved: {})",
                                    alias,
                                    path,
                                    resolved.display()
                                ),
                            ));
                        }
                    }
                }
            }

            // Check for unused tasks (not referenced in deps or with blocks)
            if suggest {
                let mut referenced: std::collections::HashSet<&str> =
                    std::collections::HashSet::new();
                for (source, target) in workflow.edges() {
                    referenced.insert(source);
                    referenced.insert(target);
                }
                for task in &workflow.tasks {
                    if let Some(ref with_spec) = task.with_spec {
                        for entry in with_spec.values() {
                            if let Some(task_ref) = entry.task_id() {
                                referenced.insert(task_ref);
                            }
                        }
                    }
                }
                for task in &workflow.tasks {
                    if !referenced.contains(task.id.as_str()) && workflow.tasks.len() > 1 {
                        // First task or leaf tasks are often not referenced
                        if workflow.tasks.first().map(|t| &t.id) != Some(&task.id) {
                            suggestions.push(format!(
                                "Task '{}' is not referenced by any other task",
                                task.id
                            ));
                        }
                    }
                }
            }

            // Output results
            let has_errors = issues.iter().any(|(level, _, _)| level == "error");
            match format.as_str() {
                "json" => {
                    let result = serde_json::json!({
                        "file": file.to_string_lossy(),
                        "valid": !has_errors,
                        "issues": issues.iter().map(|(level, code, msg)| {
                            serde_json::json!({
                                "level": level,
                                "code": code,
                                "message": msg
                            })
                        }).collect::<Vec<_>>(),
                        "suggestions": suggestions
                    });
                    println!("{}", serde_json::to_string_pretty(&result)?);

                    // JSON mode must also exit non-zero on validation errors
                    if has_errors {
                        let error_count = issues
                            .iter()
                            .filter(|(level, _, _)| level == "error")
                            .count();
                        return Err(NikaError::ValidationError {
                            reason: format!("{} validation error(s) found", error_count),
                        });
                    }
                }
                _ => {
                    // Text format
                    if issues.is_empty() {
                        if !quiet {
                            println!("{} {} is valid", "✓".green(), file.display());
                        }
                    } else {
                        for (level, code, msg) in &issues {
                            let prefix = if level == "error" {
                                "✗".red()
                            } else {
                                "⚠".yellow()
                            };
                            println!("{} [{}] {}", prefix, code.cyan(), msg);
                        }
                    }

                    if suggest && !suggestions.is_empty() {
                        println!();
                        println!("{}", "Suggestions:".cyan().bold());
                        for suggestion in &suggestions {
                            println!("  {} {}", "→".cyan(), suggestion);
                        }
                    }

                    if has_errors {
                        let error_count = issues
                            .iter()
                            .filter(|(level, _, _)| level == "error")
                            .count();
                        return Err(NikaError::ValidationError {
                            reason: format!("{} validation error(s) found", error_count),
                        });
                    }
                }
            }

            Ok(())
        }
    }
}

fn generate_ascii_dag(workflow: &nika_engine::ast::Workflow, edges: &[(&str, &str)]) -> String {
    let mut output = String::new();
    let name = "(unnamed)";
    output.push_str("┌─────────────────────────────────────────┐\n");
    output.push_str(&format!("│ DAG: {name}"));
    let padding = 40usize.saturating_sub(name.len() + 6);
    output.push_str(&" ".repeat(padding));
    output.push_str("│\n");
    output.push_str("├─────────────────────────────────────────┤\n");

    // Build task list with verb icons
    for task in &workflow.tasks {
        let verb_icon = match &task.action {
            nika_engine::ast::TaskAction::Infer { .. } => "⚡",
            nika_engine::ast::TaskAction::Exec { .. } => "📟",
            nika_engine::ast::TaskAction::Fetch { .. } => "🛰️",
            nika_engine::ast::TaskAction::Invoke { .. } => "🔌",
            nika_engine::ast::TaskAction::Agent { .. } => "🐔",
        };
        let line = format!("│ {} {}", verb_icon, task.id);
        let line_padding = 40usize.saturating_sub(task.id.len() + 4);
        output.push_str(&format!("{}{}│\n", line, " ".repeat(line_padding)));
    }

    // Show flows (includes depends_on + implicit with: edges, deduplicated)
    if !edges.is_empty() {
        output.push_str("├─────────────────────────────────────────┤\n");
        output.push_str("│ Edges:                                  │\n");
        for (source, target) in edges {
            let flow_str = format!("  {source} → {target}");
            let flow_padding = 39usize.saturating_sub(flow_str.len());
            output.push_str(&format!("│{}{}│\n", flow_str, " ".repeat(flow_padding)));
        }
    }

    output.push_str("└─────────────────────────────────────────┘\n");
    output
}

/// Generate DOT (Graphviz) DAG representation
fn generate_dot_dag(workflow: &nika_engine::ast::Workflow, edges: &[(&str, &str)]) -> String {
    let mut output = String::new();
    let name = "workflow";
    output.push_str(&format!("digraph {name} {{\n"));
    output.push_str("  rankdir=LR;\n");
    output.push_str("  node [shape=box, style=rounded];\n\n");

    // Add nodes with styling based on verb
    for task in &workflow.tasks {
        let color = match &task.action {
            nika_engine::ast::TaskAction::Infer { .. } => "lightblue",
            nika_engine::ast::TaskAction::Exec { .. } => "lightgreen",
            nika_engine::ast::TaskAction::Fetch { .. } => "lightyellow",
            nika_engine::ast::TaskAction::Invoke { .. } => "lightpink",
            nika_engine::ast::TaskAction::Agent { .. } => "plum",
        };
        output.push_str(&format!(
            "  {} [label=\"{}\", fillcolor={}, style=\"rounded,filled\"];\n",
            task.id.replace('-', "_"),
            task.id,
            color
        ));
    }

    // Add edges (includes depends_on + implicit with: edges, deduplicated)
    output.push('\n');
    for (source, target) in edges {
        output.push_str(&format!(
            "  {} -> {};\n",
            source.replace('-', "_"),
            target.replace('-', "_")
        ));
    }

    output.push_str("}\n");
    output
}

/// Generate Mermaid DAG representation
fn generate_mermaid_dag(workflow: &nika_engine::ast::Workflow, edges: &[(&str, &str)]) -> String {
    let mut output = String::new();
    output.push_str("```mermaid\ngraph LR\n");

    // Add nodes with styling
    for task in &workflow.tasks {
        let shape = match &task.action {
            nika_engine::ast::TaskAction::Infer { .. } => ("([", "])"), // Stadium
            nika_engine::ast::TaskAction::Exec { .. } => ("[", "]"),    // Rectangle
            nika_engine::ast::TaskAction::Fetch { .. } => ("{{", "}}"), // Hexagon
            nika_engine::ast::TaskAction::Invoke { .. } => ("[[", "]]"), // Subroutine
            nika_engine::ast::TaskAction::Agent { .. } => ("((", "))"), // Circle
        };
        let verb = match &task.action {
            nika_engine::ast::TaskAction::Infer { .. } => "infer",
            nika_engine::ast::TaskAction::Exec { .. } => "exec",
            nika_engine::ast::TaskAction::Fetch { .. } => "fetch",
            nika_engine::ast::TaskAction::Invoke { .. } => "invoke",
            nika_engine::ast::TaskAction::Agent { .. } => "agent",
        };
        output.push_str(&format!(
            "  {}{}{} : {}{}\n",
            task.id.replace('-', "_"),
            shape.0,
            task.id,
            verb,
            shape.1
        ));
    }

    // Add edges (includes depends_on + implicit with: edges, deduplicated)
    output.push('\n');
    for (source, target) in edges {
        output.push_str(&format!(
            "  {} --> {}\n",
            source.replace('-', "_"),
            target.replace('-', "_")
        ));
    }

    output.push_str("```\n");
    output
}
