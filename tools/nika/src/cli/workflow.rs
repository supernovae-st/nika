//! Workflow management subcommand handler

use clap::Subcommand;
use std::path::PathBuf;

use colored::Colorize;

use nika::ast::parse_workflow;
use nika::error::NikaError;

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
            // Open workflow in Studio editor
            #[cfg(feature = "tui")]
            {
                if !quiet {
                    println!(
                        "{} Opening {} in Studio editor...",
                        "→".cyan(),
                        file.display()
                    );
                }
                nika::tui::run_tui_studio(Some(file)).await
            }
            #[cfg(not(feature = "tui"))]
            {
                let _ = (file, quiet); // Suppress unused warnings
                Err(NikaError::ConfigError {
                    reason: "TUI feature not enabled. Rebuild with `--features tui`".to_string(),
                })
            }
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
                    r#"  - id: {}
    infer: "TODO: Add your prompt here"
"#,
                    task_id
                ),
                "exec" => format!(
                    r#"  - id: {}
    exec: "echo 'TODO: Add your command here'"
"#,
                    task_id
                ),
                "fetch" => format!(
                    r#"  - id: {}
    fetch:
      url: "https://example.com/api"
      method: GET
"#,
                    task_id
                ),
                "invoke" => format!(
                    r#"  - id: {}
    invoke:
      mcp: novanet
      tool: novanet_generate
      params: {{}}
"#,
                    task_id
                ),
                "agent" => format!(
                    r#"  - id: {}
    agent:
      prompt: "TODO: Add your agent prompt here"
      max_turns: 5
"#,
                    task_id
                ),
                _ => {
                    return Err(NikaError::ValidationError {
                        reason: format!(
                            "Unknown verb '{}'. Valid: infer, exec, fetch, invoke, agent",
                            task_verb
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
                    // Check for flows: or other top-level sections
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

            // Parse workflow
            let content = std::fs::read_to_string(&file)?;
            let workflow = parse_workflow(&content)?;

            // Generate graph based on format
            let graph_output = match format.as_str() {
                "ascii" => generate_ascii_dag(&workflow),
                "dot" => generate_dot_dag(&workflow),
                "mermaid" => generate_mermaid_dag(&workflow),
                _ => {
                    return Err(NikaError::ValidationError {
                        reason: format!("Unknown format '{}'. Valid: ascii, dot, mermaid", format),
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
                    println!("{}", graph_output);
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
                return Err(NikaError::WorkflowNotFound {
                    path: file.to_string_lossy().to_string(),
                });
            }

            // Parse and validate
            let content = std::fs::read_to_string(&file)?;
            let workflow = parse_workflow(&content)?;

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
                if version != "0.10" && suggest {
                    suggestions.push(format!(
                        "Consider upgrading from @{} to @0.10 for latest features",
                        version
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

            // Check for unused tasks (not referenced in flows or use blocks)
            if suggest {
                let mut referenced: std::collections::HashSet<&str> =
                    std::collections::HashSet::new();
                for flow in &workflow.flows {
                    for target in flow.target.as_vec() {
                        referenced.insert(target);
                    }
                    for source in flow.source.as_vec() {
                        referenced.insert(source);
                    }
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
            match format.as_str() {
                "json" => {
                    let result = serde_json::json!({
                        "file": file.to_string_lossy(),
                        "valid": issues.iter().all(|(level, _, _)| level != "error"),
                        "issues": issues.iter().map(|(level, code, msg)| {
                            serde_json::json!({
                                "level": level,
                                "code": code,
                                "message": msg
                            })
                        }).collect::<Vec<_>>(),
                        "suggestions": suggestions
                    });
                    println!("{}", serde_json::to_string_pretty(&result).unwrap());
                }
                _ => {
                    // Text format
                    let has_errors = issues.iter().any(|(level, _, _)| level == "error");

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
                        return Err(NikaError::ValidationError {
                            reason: format!("{} validation error(s) found", issues.len()),
                        });
                    }
                }
            }

            Ok(())
        }
    }
}

fn generate_ascii_dag(workflow: &nika::ast::Workflow) -> String {
    let mut output = String::new();
    let name = "(unnamed)";
    output.push_str("┌─────────────────────────────────────────┐\n");
    output.push_str(&format!("│ DAG: {}", name));
    let padding = 40usize.saturating_sub(name.len() + 6);
    output.push_str(&" ".repeat(padding));
    output.push_str("│\n");
    output.push_str("├─────────────────────────────────────────┤\n");

    // Build task list with verb icons
    for task in &workflow.tasks {
        let verb_icon = match &task.action {
            nika::ast::TaskAction::Infer { .. } => "⚡",
            nika::ast::TaskAction::Exec { .. } => "📟",
            nika::ast::TaskAction::Fetch { .. } => "🛰️",
            nika::ast::TaskAction::Invoke { .. } => "🔌",
            nika::ast::TaskAction::Agent { .. } => "🐔",
        };
        let line = format!("│ {} {}", verb_icon, task.id);
        let line_padding = 40usize.saturating_sub(task.id.len() + 4);
        output.push_str(&format!("{}{}│\n", line, " ".repeat(line_padding)));
    }

    // Show flows
    if !workflow.flows.is_empty() {
        output.push_str("├─────────────────────────────────────────┤\n");
        output.push_str("│ Flows:                                  │\n");
        for flow in &workflow.flows {
            let sources = flow.source.as_vec().join(", ");
            let targets = flow.target.as_vec().join(", ");
            let flow_str = format!("  {} → {}", sources, targets);
            let flow_padding = 39usize.saturating_sub(flow_str.len());
            output.push_str(&format!("│{}{}│\n", flow_str, " ".repeat(flow_padding)));
        }
    }

    output.push_str("└─────────────────────────────────────────┘\n");
    output
}

/// Generate DOT (Graphviz) DAG representation
fn generate_dot_dag(workflow: &nika::ast::Workflow) -> String {
    let mut output = String::new();
    let name = "workflow";
    output.push_str(&format!("digraph {} {{\n", name));
    output.push_str("  rankdir=LR;\n");
    output.push_str("  node [shape=box, style=rounded];\n\n");

    // Add nodes with styling based on verb
    for task in &workflow.tasks {
        let color = match &task.action {
            nika::ast::TaskAction::Infer { .. } => "lightblue",
            nika::ast::TaskAction::Exec { .. } => "lightgreen",
            nika::ast::TaskAction::Fetch { .. } => "lightyellow",
            nika::ast::TaskAction::Invoke { .. } => "lightpink",
            nika::ast::TaskAction::Agent { .. } => "plum",
        };
        output.push_str(&format!(
            "  {} [label=\"{}\", fillcolor={}, style=\"rounded,filled\"];\n",
            task.id.replace('-', "_"),
            task.id,
            color
        ));
    }

    // Add edges
    output.push('\n');
    for flow in &workflow.flows {
        for source in flow.source.as_vec() {
            for target in flow.target.as_vec() {
                output.push_str(&format!(
                    "  {} -> {};\n",
                    source.replace('-', "_"),
                    target.replace('-', "_")
                ));
            }
        }
    }

    output.push_str("}\n");
    output
}

/// Generate Mermaid DAG representation
fn generate_mermaid_dag(workflow: &nika::ast::Workflow) -> String {
    let mut output = String::new();
    output.push_str("```mermaid\ngraph LR\n");

    // Add nodes with styling
    for task in &workflow.tasks {
        let shape = match &task.action {
            nika::ast::TaskAction::Infer { .. } => ("([", "])"), // Stadium
            nika::ast::TaskAction::Exec { .. } => ("[", "]"),    // Rectangle
            nika::ast::TaskAction::Fetch { .. } => ("{{", "}}"), // Hexagon
            nika::ast::TaskAction::Invoke { .. } => ("[[", "]]"), // Subroutine
            nika::ast::TaskAction::Agent { .. } => ("((", "))"), // Circle
        };
        let verb = match &task.action {
            nika::ast::TaskAction::Infer { .. } => "infer",
            nika::ast::TaskAction::Exec { .. } => "exec",
            nika::ast::TaskAction::Fetch { .. } => "fetch",
            nika::ast::TaskAction::Invoke { .. } => "invoke",
            nika::ast::TaskAction::Agent { .. } => "agent",
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

    // Add edges
    output.push('\n');
    for flow in &workflow.flows {
        for source in flow.source.as_vec() {
            for target in flow.target.as_vec() {
                output.push_str(&format!(
                    "  {} --> {}\n",
                    source.replace('-', "_"),
                    target.replace('-', "_")
                ));
            }
        }
    }

    output.push_str("```\n");
    output
}
