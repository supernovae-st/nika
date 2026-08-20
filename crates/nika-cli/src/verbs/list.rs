// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika list` — the workflows below the invocation directory.

use std::fmt::Write as _;
use std::path::Path;

use nika_dap::inventory::{WALK_BUDGET, collect_workflow_paths};

use crate::verbs::VerbOutput;

/// List every workflow below `root`, one stable root-relative path per line.
///
/// The project file `nika.yaml` is not a workflow. Dependency, build and
/// hidden directories follow the shared workspace-inventory exclusions. A
/// partial walk refuses instead of presenting a short list as complete.
#[must_use]
pub fn run(root: &Path) -> VerbOutput {
    let mut paths = Vec::new();
    let mut budget = WALK_BUDGET;
    let truncated = collect_workflow_paths(root, root, 64, &mut budget, &mut paths);
    if truncated {
        return VerbOutput::env(
            "workflow scan incomplete — use a narrower readable directory and retry".to_owned(),
        );
    }
    paths.sort();
    let mut text = String::new();
    for path in paths {
        let _ = writeln!(text, "{}", path.display());
    }
    VerbOutput::ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_workflows_below_the_root_in_stable_order() {
        let dir = tempfile::tempdir().expect("scratch");
        std::fs::create_dir_all(dir.path().join("nested")).expect("nested");
        std::fs::create_dir_all(dir.path().join(".git")).expect("hidden");
        std::fs::write(dir.path().join("z.nika.yaml"), "nika: z\n").expect("workflow");
        std::fs::write(dir.path().join("nested/a.nika.yml"), "nika: a\n").expect("nested workflow");
        std::fs::write(dir.path().join("nika.yaml"), "nika: proj\n").expect("project file");
        std::fs::write(dir.path().join(".git/hidden.nika.yaml"), "nika: hidden\n")
            .expect("hidden workflow");

        let out = run(dir.path());

        assert_eq!(out.code, crate::verbs::exit::OK);
        assert_eq!(out.text, "nested/a.nika.yml\nz.nika.yaml\n");
    }

    #[test]
    fn an_unreadable_root_refuses_instead_of_claiming_an_empty_list() {
        let out = run(Path::new("/path/that/does/not/exist"));

        assert_eq!(out.code, crate::verbs::exit::ENV);
        assert!(out.text.contains("scan incomplete"), "{}", out.text);
    }

    #[test]
    fn an_empty_directory_is_a_successful_empty_list() {
        let dir = tempfile::tempdir().expect("scratch");

        let out = run(dir.path());

        assert_eq!(out.code, crate::verbs::exit::OK);
        assert!(out.text.is_empty(), "script output must stay empty");
    }
}
