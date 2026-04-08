// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CodeLens Handler
//!
//! Shows inline actionable buttons above workflow elements:
//! - "▶ Run Workflow" on the `tasks:` line
//! - "✓ Validate" on the `schema:` or `workflow:` line
//! - Task count summary on `tasks:` line

#[cfg(feature = "lsp")]
use tower_lsp_server::ls_types::*;

/// Compute code lenses for the document.
#[cfg(feature = "lsp")]
pub fn compute_code_lenses(text: &str, _uri: &Uri) -> Vec<CodeLens> {
    let mut lenses = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    // Pre-compute task count (avoid O(n²) scan inside loop)
    let task_count = lines
        .iter()
        .filter(|l| l.trim().starts_with("- id:"))
        .count();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let line_range = Range {
            start: Position {
                line: i as u32,
                character: 0,
            },
            end: Position {
                line: i as u32,
                character: line.chars().map(|c| c.len_utf16()).sum::<usize>() as u32,
            },
        };

        // "✓ Validate" on schema: or workflow: line
        if trimmed.starts_with("schema:") || trimmed.starts_with("workflow:") {
            lenses.push(CodeLens {
                range: line_range,
                command: Some(Command {
                    title: "✓ Validate".to_string(),
                    command: "nika.checkWorkflow".to_string(),
                    arguments: None,
                }),
                data: None,
            });
        }

        // "▶ Run" + task count on tasks: line
        if trimmed == "tasks:" {
            lenses.push(CodeLens {
                range: line_range,
                command: Some(Command {
                    title: "▶ Run Workflow".to_string(),
                    command: "nika.runWorkflow".to_string(),
                    arguments: None,
                }),
                data: None,
            });

            if task_count > 0 {
                lenses.push(CodeLens {
                    range: line_range,
                    command: Some(Command {
                        title: format!(
                            "{} task{}",
                            task_count,
                            if task_count == 1 { "" } else { "s" }
                        ),
                        command: "nika.showTasks".to_string(),
                        arguments: None,
                    }),
                    data: None,
                });
            }
        }
    }

    lenses
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "lsp")]
    use super::*;

    #[cfg(feature = "lsp")]
    fn test_uri() -> Uri {
        "file:///test.nika.yaml".parse().unwrap()
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn validate_on_schema() {
        let lenses = compute_code_lenses("schema: \"@0.12\"\n", &test_uri());
        assert!(lenses.iter().any(|l| l
            .command
            .as_ref()
            .is_some_and(|c| c.title.contains("Validate"))));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn run_on_tasks() {
        let text = "schema: \"@0.12\"\ntasks:\n  - id: step1\n    infer: x\n";
        let lenses = compute_code_lenses(text, &test_uri());
        assert!(lenses
            .iter()
            .any(|l| l.command.as_ref().is_some_and(|c| c.title.contains("Run"))));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn task_count() {
        let text =
            "tasks:\n  - id: a\n    exec: x\n  - id: b\n    exec: y\n  - id: c\n    exec: z\n";
        let lenses = compute_code_lenses(text, &test_uri());
        assert!(lenses.iter().any(|l| l
            .command
            .as_ref()
            .is_some_and(|c| c.title.contains("3 tasks"))));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn no_lenses_on_empty() {
        let lenses = compute_code_lenses("", &test_uri());
        assert!(lenses.is_empty());
    }
}
