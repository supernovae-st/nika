// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project snapshot — ONE observation of the project the session
//! stands in: the proven root (the git root, else the working directory),
//! the project file, the workflows the ONE walker lists (bounded, and
//! honest about truncation). Observed once per session, never inferred
//! from conversation memory.

use std::path::{Path, PathBuf};

/// One workflow the walker found, judged by the installed checker.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkflowSeen {
    /// The path relative to the root.
    pub path: String,
    /// The file's own name (`nika:`), when it parsed.
    pub name: Option<String>,
    /// The checker's verdict (advisory lane).
    pub clean: bool,
    /// Findings the checker counted.
    pub findings: usize,
    /// Tasks the file declares.
    pub tasks: usize,
}

/// The project the session stands in.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ProjectSnapshot {
    /// The working directory the session was opened in.
    pub cwd: PathBuf,
    /// The proven root: the git root when one holds `cwd`, else `cwd`.
    pub root: PathBuf,
    /// The git root, when one holds `cwd`.
    pub git_root: Option<PathBuf>,
    /// The project file (`nika.yaml`) that governs `cwd`, when one exists.
    pub project_file: Option<PathBuf>,
    /// The project's spend ceiling, when the project file declares one.
    pub ceiling: Option<f64>,
    /// The workflows the ONE walker listed under the root.
    pub workflows: Vec<WorkflowSeen>,
    /// The walker stopped at its budget: the list is a prefix.
    pub truncated: bool,
}

impl ProjectSnapshot {
    /// Observe the project from `cwd` — the git root walk, the project
    /// file discovery, the workflow walk. Pure reads, bounded.
    #[must_use]
    pub fn observe(cwd: &Path) -> Self {
        let cwd = cwd.to_path_buf();
        let git_root = nika_cli_host::find_git_root(&cwd).map(|(root, _)| root);
        let root = git_root.clone().unwrap_or_else(|| cwd.clone());
        let (project_file, ceiling) = match nika_vocab::project::discover(&cwd) {
            Ok(Some((path, project))) => (Some(path), project.ceiling),
            _ => (None, None),
        };
        let (facts, truncated, _, _) = nika_dap::inventory::collect_workflows(&root);
        let workflows = facts
            .into_iter()
            .map(|f| WorkflowSeen {
                path: f.path,
                name: f.workflow,
                clean: f.clean,
                findings: f.findings,
                tasks: f.tasks,
            })
            .collect();
        Self {
            cwd,
            root,
            git_root,
            project_file,
            ceiling,
            workflows,
            truncated,
        }
    }

    /// The compact facts a reasoner receives — never the tree, never a
    /// file body: where we stand, what governs it, what exists.
    #[must_use]
    pub fn facts_lines(&self) -> Vec<String> {
        let mut lines = vec![format!("root: {}", self.root.display())];
        if let Some(git) = &self.git_root {
            lines.push(format!("git root: {}", git.display()));
        }
        match &self.project_file {
            Some(path) => {
                let ceiling = self
                    .ceiling
                    .map_or(String::new(), |c| format!(" · ceiling ${c:.2}"));
                lines.push(format!("project file: {}{ceiling}", path.display()));
            }
            None => lines.push("project file: none (nika init founds one)".to_owned()),
        }
        if self.workflows.is_empty() {
            lines.push("workflows: none under the root".to_owned());
        } else {
            let clean = self.workflows.iter().filter(|w| w.clean).count();
            lines.push(format!(
                "workflows: {} under the root · {clean} clean{}",
                self.workflows.len(),
                if self.truncated {
                    " · the list is truncated"
                } else {
                    ""
                }
            ));
            for w in self.workflows.iter().take(12) {
                lines.push(format!(
                    "  · {} ({}) · {} task(s) · {}",
                    w.path,
                    w.name.as_deref().unwrap_or("unparsed"),
                    w.tasks,
                    if w.clean {
                        "clean".to_owned()
                    } else {
                        format!("{} finding(s)", w.findings)
                    }
                ));
            }
        }
        lines
    }

    /// The workflow whose path or name matches `needle` (a path suffix or
    /// the `nika:` name), when exactly one does.
    #[must_use]
    pub fn find(&self, needle: &str) -> Option<&WorkflowSeen> {
        let needle = needle.trim();
        let mut hits = self.workflows.iter().filter(|w| {
            w.path == needle
                || w.path.ends_with(&format!("/{needle}"))
                || w.name.as_deref() == Some(needle)
                || w.path.trim_end_matches(".nika.yaml") == needle
        });
        let first = hits.next()?;
        if hits.next().is_some() {
            return None;
        }
        Some(first)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("a.nika.yaml"),
            "nika: alpha\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
        )
        .expect("a");
        std::fs::write(
            dir.path().join("b.nika.yaml"),
            "nika: beta\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n",
        )
        .expect("b");
        std::fs::write(dir.path().join("nika.yaml"), "nika: demo\nceiling: 0.25\n")
            .expect("project");
        dir
    }

    /// The snapshot sees the two workflows with the checker's verdicts, the
    /// project file with its ceiling, and no git root outside a repo.
    #[test]
    fn the_snapshot_sees_the_project_once() {
        let dir = tree();
        let snap = ProjectSnapshot::observe(dir.path());
        assert_eq!(snap.workflows.len(), 2, "{snap:?}");
        assert!(snap.project_file.is_some());
        assert_eq!(snap.ceiling, Some(0.25));
        let alpha = snap.find("alpha").expect("by name");
        assert!(alpha.clean, "{alpha:?}");
        let beta = snap.find("b.nika.yaml").expect("by path");
        assert!(!beta.clean, "an exec with no grant is a finding: {beta:?}");
        let facts = snap.facts_lines();
        assert!(facts.iter().any(|l| l.starts_with("root: ")));
        assert!(
            facts.iter().any(|l| l.contains("ceiling $0.25")),
            "{facts:?}"
        );
        assert!(
            facts
                .iter()
                .any(|l| l.contains("workflows: 2 under the root · 1 clean")),
            "{facts:?}"
        );
    }

    /// An ambiguous needle finds nothing rather than guessing.
    #[test]
    fn an_ambiguous_needle_finds_nothing() {
        let dir = tree();
        let snap = ProjectSnapshot::observe(dir.path());
        assert!(snap.find("nika.yaml").is_none(), "two paths end with it");
        assert!(snap.find("nothing").is_none());
    }
}
