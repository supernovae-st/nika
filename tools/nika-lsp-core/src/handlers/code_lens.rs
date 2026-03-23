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

/// Commands that a code lens can trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LensCommand {
    /// "▶ Run Workflow" button
    Run,
    /// "✓ Validate" button
    Validate,
    /// "N tasks" info badge
    TaskCount(usize),
}

impl LensCommand {
    pub fn title(&self) -> String {
        match self {
            Self::Run => "▶ Run Workflow".into(),
            Self::Validate => "✓ Validate".into(),
            Self::TaskCount(n) => format!("{} task{}", n, if *n == 1 { "" } else { "s" }),
        }
    }

    pub fn vscode_command(&self) -> &'static str {
        match self {
            Self::Run => "nika.runWorkflow",
            Self::Validate => "nika.checkWorkflow",
            Self::TaskCount(_) => "nika.showTasks",
        }
    }
}

/// Compute code lenses for the document.
pub fn code_lenses(text: &str) -> Vec<CodeLensEntry> {
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
}
