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

#[cfg(test)]
use nika_engine::ast::analyzed::{
    AnalyzedExecAction, AnalyzedFetchAction, AnalyzedForEach, AnalyzedInferAction,
    AnalyzedInvokeAction, AnalyzedRetry, AnalyzedTask, TaskId,
};

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

            // L031: for_each without explicit concurrency
            if fe.concurrency.is_none() {
                findings.push(LintFinding {
                    severity: Severity::Info,
                    rule: "L031",
                    task_id: Some(task.name.clone()),
                    message: "for_each without explicit concurrency: — default is sequential \
                              (concurrency: 1). Consider adding concurrency: N for parallel."
                        .into(),
                });
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
    // A task is "unused" if no other task consumes its output (depends_on/implicit_deps).
    // Exempt terminal nodes: tasks that have upstream deps but no downstream consumers
    // are natural workflow outputs. Orphans (no upstream AND no downstream) are flagged.
    if workflow.tasks.len() > 1 {
        for task in &workflow.tasks {
            let is_referenced = workflow.tasks.iter().any(|other| {
                other.depends_on.contains(&task.id) || other.implicit_deps.contains(&task.id)
            });
            if is_referenced {
                continue;
            }
            // Not referenced. If the task has upstream deps, it's a terminal node
            // (end of a chain) — exempt as a natural workflow output.
            let has_upstream = !task.depends_on.is_empty() || !task.implicit_deps.is_empty();
            if has_upstream {
                continue;
            }
            // No upstream, no downstream = orphan (disconnected from DAG). Flag it.
            findings.push(LintFinding {
                severity: Severity::Warning,
                rule: "L060",
                task_id: Some(task.name.clone()),
                message: "Task output is never consumed by downstream tasks".into(),
            });
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

    // L080: expensive task after conditional without its own when:
    // If a task has when: and a downstream task (infer/agent/fetch) doesn't,
    // the downstream runs unconditionally even when the conditional was skipped.
    {
        let conditional_ids: Vec<_> = workflow
            .tasks
            .iter()
            .filter(|t| t.when.is_some())
            .map(|t| t.id)
            .collect();
        if !conditional_ids.is_empty() {
            for task in &workflow.tasks {
                if task.when.is_some() {
                    continue; // already conditional
                }
                let is_expensive = matches!(
                    task.action,
                    AnalyzedTaskAction::Infer(_)
                        | AnalyzedTaskAction::Agent(_)
                        | AnalyzedTaskAction::Fetch(_)
                );
                if !is_expensive {
                    continue;
                }
                // Check if any upstream is conditional
                let has_conditional_upstream = task
                    .depends_on
                    .iter()
                    .chain(task.implicit_deps.iter())
                    .any(|dep| conditional_ids.contains(dep));
                if has_conditional_upstream {
                    findings.push(LintFinding {
                        severity: Severity::Info,
                        rule: "L080",
                        task_id: Some(task.name.clone()),
                        message: "Expensive task depends on conditional — consider adding `when:`"
                            .into(),
                    });
                }
            }
        }
    }

    // L090: duplicate task IDs (analyzer catches this as an error, but lint
    // gives a friendlier message before the hard error)
    {
        let mut seen = std::collections::HashSet::new();
        for task in &workflow.tasks {
            if !seen.insert(&task.name) {
                findings.push(LintFinding {
                    severity: Severity::Warning,
                    rule: "L090",
                    task_id: Some(task.name.clone()),
                    message: "Duplicate task name — each task must have a unique ID".into(),
                });
            }
        }
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

    // Capture analyzer warnings before they're lost
    let analyzer_warnings: Vec<String> = result
        .warnings
        .iter()
        .map(|w| {
            let code = w.kind.code();
            if let Some(ref suggestion) = w.suggestion {
                format!("[{}] {} ({})", code, w.message, suggestion)
            } else {
                format!("[{}] {}", code, w.message)
            }
        })
        .collect();

    if !result.errors.is_empty() {
        for err in &result.errors {
            eprintln!("  {} {}", "error:".red().bold(), err.message);
        }
        return Err(NikaError::BuiltinToolError {
            tool: "lint".into(),
            reason: format!("{} analysis error(s) found", result.errors.len()),
        });
    }

    let workflow = result.value.ok_or_else(|| NikaError::BuiltinToolError {
        tool: "lint".into(),
        reason: "Internal error: analyzer produced no value".into(),
    })?;
    let findings = lint_workflow(&workflow);

    let lint_warnings = findings
        .iter()
        .filter(|f| f.severity == Severity::Warning)
        .count();
    let infos = findings
        .iter()
        .filter(|f| f.severity == Severity::Info)
        .count();

    // Display analyzer warnings first
    if !analyzer_warnings.is_empty() && !quiet {
        eprintln!(
            "  {} {}",
            "\u{26A0}".yellow().bold(),
            format!("{} analyzer warning(s):", analyzer_warnings.len())
                .yellow()
                .bold()
        );
        for w in &analyzer_warnings {
            eprintln!("     {} {}", "\u{2502}".dimmed(), w.yellow());
        }
        eprintln!();
    }

    if findings.is_empty() && analyzer_warnings.is_empty() {
        if !quiet {
            eprintln!(
                "  {} {} — no lint issues found",
                "CLEAN".green().bold(),
                file
            );
        }
    } else if findings.is_empty() {
        // Only analyzer warnings, no lint findings
        if !quiet {
            eprintln!(
                "  {} {} — no lint issues, {} analyzer warning(s) in {}",
                "Lint:".cyan(),
                0,
                analyzer_warnings.len(),
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
                lint_warnings,
                infos,
                file
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_core::binding::WithSpec;
    use nika_core::source::Span;

    /// Helper: create a minimal task with the given action
    fn make_task(
        workflow: &mut AnalyzedWorkflow,
        name: &str,
        action: AnalyzedTaskAction,
    ) -> TaskId {
        let id = workflow.task_table.insert(name);
        workflow.tasks.push(AnalyzedTask {
            id,
            name: name.to_string(),
            description: None,
            action,
            provider: None,
            model: None,

            with_spec: WithSpec::default(),
            depends_on: Vec::new(),
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
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
            when: None,
            span: Span::dummy(),
        });
        id
    }

    fn has_rule(findings: &[LintFinding], rule: &str) -> bool {
        findings.iter().any(|f| f.rule == rule)
    }

    fn has_rule_for(findings: &[LintFinding], rule: &str, task: &str) -> bool {
        findings
            .iter()
            .any(|f| f.rule == rule && f.task_id.as_deref() == Some(task))
    }

    // ── L001: missing workflow description ──────────────────────

    #[test]
    fn l001_fires_when_no_description() {
        let mut wf = AnalyzedWorkflow::default();
        wf.description = None;
        make_task(&mut wf, "t1", AnalyzedTaskAction::default());
        let findings = lint_workflow(&wf);
        assert!(has_rule(&findings, "L001"), "L001 should fire");
    }

    #[test]
    fn l001_silent_when_description_present() {
        let mut wf = AnalyzedWorkflow::default();
        wf.description = Some("A good workflow".into());
        make_task(&mut wf, "t1", AnalyzedTaskAction::default());
        let findings = lint_workflow(&wf);
        assert!(!has_rule(&findings, "L001"), "L001 should NOT fire");
    }

    // ── L002: missing workflow name ─────────────────────────────

    #[test]
    fn l002_fires_when_no_name() {
        let mut wf = AnalyzedWorkflow::default();
        wf.name = None;
        make_task(&mut wf, "t1", AnalyzedTaskAction::default());
        let findings = lint_workflow(&wf);
        assert!(has_rule(&findings, "L002"), "L002 should fire");
    }

    #[test]
    fn l002_silent_when_name_present() {
        let mut wf = AnalyzedWorkflow::default();
        wf.name = Some("my-workflow".into());
        make_task(&mut wf, "t1", AnalyzedTaskAction::default());
        let findings = lint_workflow(&wf);
        assert!(!has_rule(&findings, "L002"), "L002 should NOT fire");
    }

    // ── L010: missing task description ──────────────────────────

    #[test]
    fn l010_fires_on_task_without_description() {
        let mut wf = AnalyzedWorkflow::default();
        make_task(&mut wf, "t1", AnalyzedTaskAction::default());
        let findings = lint_workflow(&wf);
        assert!(has_rule_for(&findings, "L010", "t1"));
    }

    #[test]
    fn l010_silent_when_task_has_description() {
        let mut wf = AnalyzedWorkflow::default();
        let id = make_task(&mut wf, "t1", AnalyzedTaskAction::default());
        wf.tasks
            .iter_mut()
            .find(|t| t.id == id)
            .unwrap()
            .description = Some("Does something".into());
        let findings = lint_workflow(&wf);
        assert!(!has_rule_for(&findings, "L010", "t1"));
    }

    // ── L020: fetch/invoke without retry ────────────────────────

    #[test]
    fn l020_fires_on_fetch_without_retry() {
        let mut wf = AnalyzedWorkflow::default();
        make_task(
            &mut wf,
            "scrape",
            AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
        );
        let findings = lint_workflow(&wf);
        assert!(has_rule_for(&findings, "L020", "scrape"));
    }

    #[test]
    fn l020_fires_on_invoke_without_retry() {
        let mut wf = AnalyzedWorkflow::default();
        make_task(
            &mut wf,
            "call_tool",
            AnalyzedTaskAction::Invoke(AnalyzedInvokeAction::default()),
        );
        let findings = lint_workflow(&wf);
        assert!(has_rule_for(&findings, "L020", "call_tool"));
    }

    #[test]
    fn l020_silent_when_retry_present() {
        let mut wf = AnalyzedWorkflow::default();
        let id = make_task(
            &mut wf,
            "scrape",
            AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
        );
        wf.tasks.iter_mut().find(|t| t.id == id).unwrap().retry = Some(AnalyzedRetry {
            max_attempts: 3,
            delay_ms: 1000,
            backoff: None,
            span: Span::dummy(),
        });
        let findings = lint_workflow(&wf);
        assert!(!has_rule_for(&findings, "L020", "scrape"));
    }

    #[test]
    fn l020_silent_on_infer_without_retry() {
        let mut wf = AnalyzedWorkflow::default();
        make_task(
            &mut wf,
            "gen",
            AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
        );
        let findings = lint_workflow(&wf);
        assert!(!has_rule_for(&findings, "L020", "gen"));
    }

    // ── L030: high concurrency ──────────────────────────────────

    #[test]
    fn l030_fires_on_concurrency_above_10() {
        let mut wf = AnalyzedWorkflow::default();
        let id = make_task(&mut wf, "batch", AnalyzedTaskAction::default());
        wf.tasks.iter_mut().find(|t| t.id == id).unwrap().for_each = Some(AnalyzedForEach {
            items: "$data".into(),
            as_var: "item".into(),
            concurrency: Some(15),
            fail_fast: true,
            span: Span::dummy(),
        });
        let findings = lint_workflow(&wf);
        assert!(has_rule_for(&findings, "L030", "batch"));
    }

    #[test]
    fn l030_silent_on_concurrency_5() {
        let mut wf = AnalyzedWorkflow::default();
        let id = make_task(&mut wf, "batch", AnalyzedTaskAction::default());
        wf.tasks.iter_mut().find(|t| t.id == id).unwrap().for_each = Some(AnalyzedForEach {
            items: "$data".into(),
            as_var: "item".into(),
            concurrency: Some(5),
            fail_fast: true,
            span: Span::dummy(),
        });
        let findings = lint_workflow(&wf);
        assert!(!has_rule_for(&findings, "L030", "batch"));
    }

    // ── L050: agent task info ───────────────────────────────────

    #[test]
    fn l050_fires_on_agent_task() {
        let mut wf = AnalyzedWorkflow::default();
        make_task(
            &mut wf,
            "assistant",
            AnalyzedTaskAction::Agent(Box::default()),
        );
        let findings = lint_workflow(&wf);
        assert!(has_rule_for(&findings, "L050", "assistant"));
    }

    // ── L060: unused task ───────────────────────────────────────

    #[test]
    fn l060_fires_on_disconnected_tasks() {
        let mut wf = AnalyzedWorkflow::default();
        let _id1 = make_task(&mut wf, "orphan_a", AnalyzedTaskAction::default());
        let _id2 = make_task(&mut wf, "orphan_b", AnalyzedTaskAction::default());
        // Neither depends on the other → both are orphans
        let findings = lint_workflow(&wf);
        assert!(
            has_rule_for(&findings, "L060", "orphan_a"),
            "L060 should fire on disconnected task"
        );
        assert!(
            has_rule_for(&findings, "L060", "orphan_b"),
            "L060 should fire on disconnected task"
        );
    }

    #[test]
    fn l060_silent_when_task_is_referenced() {
        let mut wf = AnalyzedWorkflow::default();
        let id1 = make_task(&mut wf, "step1", AnalyzedTaskAction::default());
        let _id2 = make_task(&mut wf, "step2", AnalyzedTaskAction::default());
        // step2 depends_on step1 → step1 is used
        wf.tasks
            .iter_mut()
            .find(|t| t.name == "step2")
            .unwrap()
            .depends_on = vec![id1];
        let findings = lint_workflow(&wf);
        assert!(!has_rule_for(&findings, "L060", "step1"));
    }

    #[test]
    fn l060_exempts_all_leaf_nodes() {
        // After fix: ALL leaf nodes (no downstream) are exempt, not just the last.
        // In a fan-out pattern, both leaf tasks should be exempt.
        let mut wf = AnalyzedWorkflow::default();
        let id1 = make_task(&mut wf, "root", AnalyzedTaskAction::default());
        let _id2 = make_task(&mut wf, "leaf_a", AnalyzedTaskAction::default());
        let _id3 = make_task(&mut wf, "leaf_b", AnalyzedTaskAction::default());
        // leaf_a and leaf_b both depend on root
        for name in ["leaf_a", "leaf_b"] {
            wf.tasks
                .iter_mut()
                .find(|t| t.name == name)
                .unwrap()
                .depends_on = vec![id1];
        }
        let findings = lint_workflow(&wf);
        // Neither leaf should trigger L060 — they're natural terminal nodes
        assert!(!has_rule_for(&findings, "L060", "leaf_a"));
        assert!(!has_rule_for(&findings, "L060", "leaf_b"));
        // root IS referenced, so it shouldn't fire either
        assert!(!has_rule_for(&findings, "L060", "root"));
    }

    #[test]
    fn l060_fires_on_orphan_even_if_last() {
        // Orphan task (no deps in, no deps out) should be flagged even if last.
        let mut wf = AnalyzedWorkflow::default();
        let id1 = make_task(&mut wf, "step1", AnalyzedTaskAction::default());
        let _id2 = make_task(&mut wf, "step2", AnalyzedTaskAction::default());
        let _id3 = make_task(&mut wf, "orphan_last", AnalyzedTaskAction::default());
        // step2 depends on step1. orphan_last is isolated.
        wf.tasks
            .iter_mut()
            .find(|t| t.name == "step2")
            .unwrap()
            .depends_on = vec![id1];
        let findings = lint_workflow(&wf);
        // orphan_last is unreferenced AND has no deps → L060 should fire
        assert!(
            has_rule_for(&findings, "L060", "orphan_last"),
            "L060 should fire on orphan task even if last"
        );
    }

    // ── L070: single-task workflow ──────────────────────────────

    #[test]
    fn l070_fires_on_single_task_workflow() {
        let mut wf = AnalyzedWorkflow::default();
        make_task(&mut wf, "only", AnalyzedTaskAction::default());
        let findings = lint_workflow(&wf);
        assert!(has_rule(&findings, "L070"));
    }

    #[test]
    fn l070_silent_on_multi_task_workflow() {
        let mut wf = AnalyzedWorkflow::default();
        let id1 = make_task(&mut wf, "t1", AnalyzedTaskAction::default());
        let _id2 = make_task(&mut wf, "t2", AnalyzedTaskAction::default());
        wf.tasks
            .iter_mut()
            .find(|t| t.name == "t2")
            .unwrap()
            .depends_on = vec![id1];
        let findings = lint_workflow(&wf);
        assert!(!has_rule(&findings, "L070"));
    }

    // ── Clean workflow (no findings) ────────────────────────────

    #[test]
    fn clean_workflow_produces_no_warnings() {
        let mut wf = AnalyzedWorkflow::default();
        wf.name = Some("good-workflow".into());
        wf.description = Some("A well-formed workflow".into());

        let id1 = make_task(
            &mut wf,
            "fetch_data",
            AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
        );
        // Give it description + retry
        {
            let t = wf.tasks.iter_mut().find(|t| t.id == id1).unwrap();
            t.description = Some("Fetches data".into());
            t.retry = Some(AnalyzedRetry {
                max_attempts: 3,
                delay_ms: 1000,
                backoff: None,
                span: Span::dummy(),
            });
        }

        let _id2 = make_task(&mut wf, "process", AnalyzedTaskAction::default());
        {
            let t = wf.tasks.iter_mut().find(|t| t.name == "process").unwrap();
            t.description = Some("Processes data".into());
            t.depends_on = vec![id1]; // references fetch_data
        }

        let findings = lint_workflow(&wf);
        let warnings: Vec<_> = findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .collect();
        assert!(
            warnings.is_empty(),
            "Expected 0 warnings, got: {warnings:?}"
        );
    }

    // ── L080: expensive task after conditional ──────────────────

    #[test]
    fn l080_fires_on_infer_after_conditional() {
        let mut wf = AnalyzedWorkflow::default();
        let id1 = make_task(&mut wf, "check", AnalyzedTaskAction::default());
        wf.tasks.iter_mut().find(|t| t.id == id1).unwrap().when = Some("{{inputs.enabled}}".into());

        let _id2 = make_task(
            &mut wf,
            "generate",
            AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
        );
        wf.tasks
            .iter_mut()
            .find(|t| t.name == "generate")
            .unwrap()
            .depends_on = vec![id1];

        let findings = lint_workflow(&wf);
        assert!(has_rule_for(&findings, "L080", "generate"));
    }

    #[test]
    fn l080_silent_when_downstream_also_has_when() {
        let mut wf = AnalyzedWorkflow::default();
        let id1 = make_task(&mut wf, "check", AnalyzedTaskAction::default());
        wf.tasks.iter_mut().find(|t| t.id == id1).unwrap().when = Some("{{inputs.enabled}}".into());

        let _id2 = make_task(
            &mut wf,
            "generate",
            AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
        );
        {
            let t = wf.tasks.iter_mut().find(|t| t.name == "generate").unwrap();
            t.depends_on = vec![id1];
            t.when = Some("{{inputs.enabled}}".into());
        }

        let findings = lint_workflow(&wf);
        assert!(!has_rule_for(&findings, "L080", "generate"));
    }

    #[test]
    fn l080_silent_on_exec_after_conditional() {
        // exec is not "expensive" in the L080 sense (no LLM cost)
        let mut wf = AnalyzedWorkflow::default();
        let id1 = make_task(&mut wf, "check", AnalyzedTaskAction::default());
        wf.tasks.iter_mut().find(|t| t.id == id1).unwrap().when = Some("{{inputs.enabled}}".into());

        let _id2 = make_task(
            &mut wf,
            "run_cmd",
            AnalyzedTaskAction::Exec(AnalyzedExecAction::default()),
        );
        wf.tasks
            .iter_mut()
            .find(|t| t.name == "run_cmd")
            .unwrap()
            .depends_on = vec![id1];

        let findings = lint_workflow(&wf);
        assert!(!has_rule_for(&findings, "L080", "run_cmd"));
    }

    // ── L090: duplicate task names ──────────────────────────────

    #[test]
    fn l090_fires_on_duplicate_names() {
        let mut wf = AnalyzedWorkflow::default();
        // Manually insert two tasks with the same name (bypassing TaskTable uniqueness)
        let id1 = wf.task_table.insert("step1");
        wf.tasks.push(AnalyzedTask {
            id: id1,
            name: "step1".to_string(),
            description: None,
            action: AnalyzedTaskAction::default(),
            provider: None,
            model: None,

            with_spec: WithSpec::default(),
            depends_on: Vec::new(),
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
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
            when: None,
            span: Span::dummy(),
        });
        // Second task with same name but different TaskId
        let id2 = TaskId::new(99);
        wf.tasks.push(AnalyzedTask {
            id: id2,
            name: "step1".to_string(), // duplicate!
            description: None,
            action: AnalyzedTaskAction::default(),
            provider: None,
            model: None,

            with_spec: WithSpec::default(),
            depends_on: Vec::new(),
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
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
            when: None,
            span: Span::dummy(),
        });

        let findings = lint_workflow(&wf);
        assert!(has_rule_for(&findings, "L090", "step1"));
    }

    #[test]
    fn l090_silent_on_unique_names() {
        let mut wf = AnalyzedWorkflow::default();
        make_task(&mut wf, "step1", AnalyzedTaskAction::default());
        make_task(&mut wf, "step2", AnalyzedTaskAction::default());
        let findings = lint_workflow(&wf);
        assert!(!has_rule(&findings, "L090"));
    }

    // ── L031: for_each without concurrency ─────────────────────────

    #[test]
    fn l031_fires_when_no_concurrency() {
        let mut wf = AnalyzedWorkflow::default();
        let id = make_task(&mut wf, "batch", AnalyzedTaskAction::default());
        wf.tasks.iter_mut().find(|t| t.id == id).unwrap().for_each = Some(AnalyzedForEach {
            items: "$data".into(),
            as_var: "item".into(),
            concurrency: None,
            fail_fast: true,
            span: Span::dummy(),
        });
        let findings = lint_workflow(&wf);
        assert!(has_rule_for(&findings, "L031", "batch"), "L031 should fire");
    }

    #[test]
    fn l031_silent_with_concurrency() {
        let mut wf = AnalyzedWorkflow::default();
        let id = make_task(&mut wf, "batch", AnalyzedTaskAction::default());
        wf.tasks.iter_mut().find(|t| t.id == id).unwrap().for_each = Some(AnalyzedForEach {
            items: "$data".into(),
            as_var: "item".into(),
            concurrency: Some(5),
            fail_fast: true,
            span: Span::dummy(),
        });
        let findings = lint_workflow(&wf);
        assert!(
            !has_rule_for(&findings, "L031", "batch"),
            "L031 should NOT fire with concurrency"
        );
    }
}
