//! `nika lint` — Best-practice linting for .nika.yaml workflows
//!
//! Goes beyond `nika check` (syntax + DAG) to detect quality issues:
//! - Missing descriptions
//! - Unused tasks (no downstream consumers)
//! - Missing error handling (no retry on fetch/invoke)
//! - Performance hints (high concurrency)

use colored::Colorize;
use nika_engine::ast::analyzed::{AnalyzedTaskAction, AnalyzedWorkflow};
use nika_engine::error::NikaError;
use std::path::Path;

/// Lint severity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Info,
}

/// A single lint finding
#[derive(Debug)]
pub struct LintFinding {
    pub severity: Severity,
    pub rule: &'static str,
    pub task_id: Option<String>,
    pub message: String,
}

impl std::fmt::Display for LintFinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let icon = match self.severity {
            Severity::Warning => "⚠",
            Severity::Info => "ℹ",
        };
        if let Some(ref task) = self.task_id {
            write!(f, "  {} [{}] {}: {}", icon, self.rule, task, self.message)
        } else {
            write!(f, "  {} [{}] {}", icon, self.rule, self.message)
        }
    }
}

/// Run all lint rules on an analyzed workflow.
pub fn lint_workflow(workflow: &AnalyzedWorkflow) -> Vec<LintFinding> {
    let mut findings = Vec::new();

    // Workflow-level rules
    if workflow.description.is_none() {
        findings.push(LintFinding {
            severity: Severity::Info,
            rule: "L001",
            task_id: None,
            message: "Workflow has no description — add `description:` for documentation".into(),
        });
    }
    if workflow.name.is_none() {
        findings.push(LintFinding {
            severity: Severity::Info,
            rule: "L002",
            task_id: None,
            message: "Workflow has no name — add `workflow:` for identification".into(),
        });
    }

    // Task-level rules
    for task in &workflow.tasks {
        // L010: missing description
        if task.description.is_none() {
            findings.push(LintFinding {
                severity: Severity::Info,
                rule: "L010",
                task_id: Some(task.name.clone()),
                message: "Task has no description".into(),
            });
        }

        // L020: fetch/invoke without retry
        let needs_retry = matches!(
            task.action,
            AnalyzedTaskAction::Fetch(_) | AnalyzedTaskAction::Invoke(_)
        );
        if needs_retry && task.retry.is_none() {
            findings.push(LintFinding {
                severity: Severity::Warning,
                rule: "L020",
                task_id: Some(task.name.clone()),
                message: "fetch/invoke without retry: — network calls can fail transiently".into(),
            });
        }

        // L030: high concurrency
        if let Some(ref fe) = task.for_each {
            if let Some(c) = fe.concurrency {
                if c > 10 {
                    findings.push(LintFinding {
                        severity: Severity::Warning,
                        rule: "L030",
                        task_id: Some(task.name.clone()),
                        message: format!(
                            "concurrency: {} is high — risk of rate limiting (consider ≤5)",
                            c
                        ),
                    });
                }
            }
        }

        // L050: agent without explicit max_turns check (info)
        if matches!(task.action, AnalyzedTaskAction::Agent(_)) {
            findings.push(LintFinding {
                severity: Severity::Info,
                rule: "L050",
                task_id: Some(task.name.clone()),
                message: "agent: task — ensure max_turns and token_budget are set".into(),
            });
        }
    }

    // L060: unused tasks (not referenced by downstream)
    // Compare by TaskId since depends_on/implicit_deps use interned IDs
    if workflow.tasks.len() > 1 {
        let last_id = workflow.tasks.last().map(|t| t.id);
        for task in &workflow.tasks {
            if Some(task.id) == last_id {
                continue;
            }
            let is_referenced = workflow.tasks.iter().any(|other| {
                other.depends_on.contains(&task.id) || other.implicit_deps.contains(&task.id)
            });
            if !is_referenced {
                findings.push(LintFinding {
                    severity: Severity::Warning,
                    rule: "L060",
                    task_id: Some(task.name.clone()),
                    message: "Task output is never consumed by downstream tasks".into(),
                });
            }
        }
    }

    // L070: single-task workflow
    if workflow.tasks.len() == 1 {
        findings.push(LintFinding {
            severity: Severity::Info,
            rule: "L070",
            task_id: None,
            message: "Single-task workflow — consider `nika infer` or `nika fetch` directly".into(),
        });
    }

    findings
}

/// Handle the `nika lint` CLI command.
pub async fn handle_lint_command(file: &str, quiet: bool) -> Result<(), NikaError> {
    let path = Path::new(file);
    if !path.exists() {
        return Err(NikaError::WorkflowNotFound {
            path: file.to_string(),
        });
    }

    let yaml = tokio::fs::read_to_string(path).await?;

    let validator = nika_engine::ast::schema_validator::WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    let base_path = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    let raw = nika_core::ast::raw::parse(&yaml, nika_core::source::FileId(0)).map_err(|e| {
        NikaError::BuiltinToolError {
            tool: "lint".into(),
            reason: format!("Parse error: {e}"),
        }
    })?;
    let include_raw = nika_engine::ast::expand_raw_include(raw, base_path)?;
    let result = nika_core::ast::analyzer::analyze(include_raw);

    if !result.errors.is_empty() {
        for err in &result.errors {
            eprintln!("  {} {}", "error:".red().bold(), err.message);
        }
        return Err(NikaError::BuiltinToolError {
            tool: "lint".into(),
            reason: format!("{} analysis error(s) found", result.errors.len()),
        });
    }

    let workflow = result.value.unwrap();
    let findings = lint_workflow(&workflow);

    let warnings = findings
        .iter()
        .filter(|f| f.severity == Severity::Warning)
        .count();
    let infos = findings
        .iter()
        .filter(|f| f.severity == Severity::Info)
        .count();

    if findings.is_empty() {
        if !quiet {
            eprintln!(
                "  {} {} — no lint issues found",
                "CLEAN".green().bold(),
                file
            );
        }
    } else {
        for finding in &findings {
            let colored_msg = match finding.severity {
                Severity::Warning => finding.to_string().yellow().to_string(),
                Severity::Info => finding.to_string().dimmed().to_string(),
            };
            eprintln!("{colored_msg}");
        }
        eprintln!();
        if !quiet {
            eprintln!(
                "  {} {} warning(s), {} info(s) in {}",
                "Lint:".cyan(),
                warnings,
                infos,
                file
            );
        }
    }

    Ok(())
}
