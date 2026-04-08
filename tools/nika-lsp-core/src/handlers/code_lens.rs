// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CodeLens handler — inline actionable buttons.
//!
//! Protocol-agnostic: returns `Vec<CodeLensEntry>` with line + command.
//! The tower-lsp shim converts to `CodeLens`.

/// Protocol-agnostic code lens entry.
#[derive(Debug, Clone)]
pub struct CodeLensEntry {
    pub line: u32,
    pub command: LensCommand,
}

use nika_core::catalogs::WorkflowRunInfo;

/// Commands that a code lens can trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LensCommand {
    /// "▶ Run Workflow" button
    Run,
    /// "✓ Validate" button
    Validate,
    /// "N tasks" info badge
    TaskCount(usize),
    /// Last run status: "✓ Last: 2.3s (12s ago)" or "✗ FAILED (1h ago)"
    LastRun { status: String, detail: String },
}

impl LensCommand {
    pub fn title(&self) -> String {
        match self {
            Self::Run => "▶ Run Workflow".into(),
            Self::Validate => "✓ Validate".into(),
            Self::TaskCount(n) => format!("{} task{}", n, if *n == 1 { "" } else { "s" }),
            Self::LastRun { status, detail } => format!("{status} {detail}"),
        }
    }

    pub fn vscode_command(&self) -> &'static str {
        match self {
            Self::Run => "nika.runWorkflow",
            Self::Validate => "nika.checkWorkflow",
            Self::TaskCount(_) => "nika.showTasks",
            Self::LastRun { .. } => "nika.showTasks",
        }
    }
}

/// Compute code lenses for the document.
///
/// `last_run` is optionally passed from daemon history for the "Last: ..." lens.
pub fn code_lenses(text: &str, last_run: Option<&WorkflowRunInfo>) -> Vec<CodeLensEntry> {
    let mut lenses = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    let task_count = lines
        .iter()
        .filter(|l| l.trim().starts_with("- id:"))
        .count();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("schema:") || trimmed.starts_with("workflow:") {
            lenses.push(CodeLensEntry {
                line: i as u32,
                command: LensCommand::Validate,
            });

            // Add last run lens on the schema/workflow line
            if let Some(run) = last_run {
                let status = if run.exit_code == Some(0) {
                    "\u{2713}".to_string() // ✓
                } else if run.exit_code.is_some() {
                    "\u{2717}".to_string() // ✗
                } else {
                    "\u{23F3}".to_string() // ⏳
                };
                let detail = format!("Last: {}", run.state);
                lenses.push(CodeLensEntry {
                    line: i as u32,
                    command: LensCommand::LastRun { status, detail },
                });
            }
        }

        if trimmed == "tasks:" {
            lenses.push(CodeLensEntry {
                line: i as u32,
                command: LensCommand::Run,
            });
            if task_count > 0 {
                lenses.push(CodeLensEntry {
                    line: i as u32,
                    command: LensCommand::TaskCount(task_count),
                });
            }
        }
    }

    lenses
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lens_command_titles() {
        assert_eq!(LensCommand::Run.title(), "▶ Run Workflow");
        assert_eq!(LensCommand::TaskCount(1).title(), "1 task");
        assert_eq!(LensCommand::TaskCount(5).title(), "5 tasks");
    }

    #[test]
    fn code_lenses_without_last_run() {
        let text = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: step1\n    infer: \"test\"";
        let lenses = code_lenses(text, None);
        assert!(lenses.iter().any(|l| l.command == LensCommand::Validate));
        assert!(lenses.iter().any(|l| l.command == LensCommand::Run));
        assert!(!lenses
            .iter()
            .any(|l| matches!(&l.command, LensCommand::LastRun { .. })));
    }

    #[test]
    fn code_lenses_with_last_run_success() {
        let run = WorkflowRunInfo {
            job_id: "j1".into(),
            state: "completed".into(),
            workflow: "test.nika.yaml".into(),
            created_at: "2026-03-27T12:00:00Z".into(),
            started_at: Some("2026-03-27T12:00:01Z".into()),
            completed_at: Some("2026-03-27T12:00:03Z".into()),
            exit_code: Some(0),
        };
        let text = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: step1\n    infer: \"test\"";
        let lenses = code_lenses(text, Some(&run));
        let last_run = lenses
            .iter()
            .find(|l| matches!(&l.command, LensCommand::LastRun { .. }));
        assert!(last_run.is_some(), "Should have LastRun lens");
        let title = last_run.unwrap().command.title();
        assert!(title.contains("✓"), "Success should show ✓: {title}");
    }

    #[test]
    fn code_lenses_with_last_run_failure() {
        let run = WorkflowRunInfo {
            job_id: "j2".into(),
            state: "failed".into(),
            workflow: "test.nika.yaml".into(),
            created_at: "2026-03-27T12:00:00Z".into(),
            started_at: Some("2026-03-27T12:00:01Z".into()),
            completed_at: Some("2026-03-27T12:00:05Z".into()),
            exit_code: Some(1),
        };
        let text = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: step1\n    infer: \"test\"";
        let lenses = code_lenses(text, Some(&run));
        let last_run = lenses
            .iter()
            .find(|l| matches!(&l.command, LensCommand::LastRun { .. }));
        assert!(last_run.is_some());
        let title = last_run.unwrap().command.title();
        assert!(title.contains("✗"), "Failure should show ✗: {title}");
    }
}
