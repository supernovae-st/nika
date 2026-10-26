// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The paths a permission ask names, in the workflow's own vocabulary (NIK-266).
//!
//! `permits.fs` globs are authored relative to the project (`./out/**`), while a harness names
//! the paths it will touch absolutely (Codex and Claude Code edit asks carry the full path).
//! The boundary predicate never matches an absolute path against a relative glob (a host path
//! is not a workspace path), so an ask INSIDE the grants read as outside them. Each location
//! is therefore projected onto the session root: inside it, root-relative; outside it,
//! absolute (only an absolute grant can admit it). And because the harness, not Nika, will
//! perform the action, a path whose effective identity differs — a symlink on the way — adds
//! that identity as a location too: the bridge requires EVERY location to fit, so a link that
//! leads out of the grants is judged where it leads (the NEP-0009 identity law, harness side).

use std::path::{Component, Path, PathBuf};

/// Fold `.` and `..` lexically (`..` never climbs above the root of an absolute path).
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// `abs` relative to the first root it sits under, else `abs` itself.
fn project(abs: &Path, roots: &[&Path]) -> String {
    for root in roots {
        if let Ok(rel) = abs.strip_prefix(root) {
            let rel = rel.to_string_lossy();
            return if rel.is_empty() {
                ".".to_owned()
            } else {
                rel.into_owned()
            };
        }
    }
    abs.to_string_lossy().into_owned()
}

/// `std::fs::canonicalize` off the async executor (a filesystem call may block).
async fn canonical(path: &Path) -> Option<PathBuf> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || std::fs::canonicalize(path).ok())
        .await
        .ok()
        .flatten()
}

/// The effective identity of `abs`: its longest existing ancestor resolved, the rest appended.
async fn effective(abs: &Path) -> PathBuf {
    let mut existing = abs.to_path_buf();
    let mut tail = Vec::new();
    loop {
        if let Some(real) = canonical(&existing).await {
            let mut out = real;
            for name in tail.iter().rev() {
                out.push(name);
            }
            return out;
        }
        match (
            existing.file_name().map(ToOwned::to_owned),
            existing.parent(),
        ) {
            (Some(name), Some(parent)) => {
                tail.push(name);
                existing = parent.to_path_buf();
            }
            _ => return abs.to_path_buf(),
        }
    }
}

/// Project the wire's `locations` onto the session `root` (see the module doc): each named
/// path, root-relative inside the root, then its effective identity when that differs.
pub(super) async fn anchor(locations: Vec<String>, root: &Path) -> Vec<String> {
    if locations.is_empty() {
        return locations;
    }
    let lexical_root = lexical(root);
    let real_root = canonical(&lexical_root).await;
    let roots: Vec<&Path> = std::iter::once(lexical_root.as_path())
        .chain(real_root.as_deref())
        .collect();
    let mut out = Vec::with_capacity(locations.len());
    for named in locations {
        let path = Path::new(&named);
        let abs = lexical(&if path.is_absolute() {
            path.to_path_buf()
        } else {
            lexical_root.join(path)
        });
        let projected = project(&abs, &roots);
        let identity = project(&effective(&abs).await, &roots);
        let moved = identity != projected;
        out.push(projected);
        if moved {
            out.push(identity);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn locations_inside_the_root_become_root_relative_and_outside_stay_absolute() {
        let root = Path::new("/nika-anchor-test-root/project");
        let got = anchor(
            vec![
                "/nika-anchor-test-root/project/out/report.md".to_owned(),
                "out/../src/lib.rs".to_owned(),
                "/nika-anchor-test-root/project".to_owned(),
                "/nika-anchor-test-root/elsewhere/x".to_owned(),
                "../escape.txt".to_owned(),
            ],
            root,
        )
        .await;
        assert_eq!(
            got,
            [
                "out/report.md",
                "src/lib.rs",
                ".",
                "/nika-anchor-test-root/elsewhere/x",
                "/nika-anchor-test-root/escape.txt",
            ]
        );
    }

    /// A symlink inside the project that leads out of it: the named path is root-relative,
    /// and its effective identity — outside — is judged too.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_symlink_that_leaves_the_root_adds_where_it_leads() {
        let world = tempfile::tempdir().expect("tempdir");
        let base = std::fs::canonicalize(world.path()).expect("real base");
        let (project, outside) = (base.join("project"), base.join("outside"));
        std::fs::create_dir_all(&project).expect("project");
        std::fs::create_dir_all(&outside).expect("outside");
        std::os::unix::fs::symlink(&outside, project.join("out")).expect("link");
        let got = anchor(
            vec![
                project.join("out/keys").to_string_lossy().into_owned(),
                "src/main.rs".to_owned(),
            ],
            &project,
        )
        .await;
        assert_eq!(
            got,
            [
                "out/keys".to_owned(),
                outside.join("keys").to_string_lossy().into_owned(),
                "src/main.rs".to_owned(),
            ]
        );
    }

    /// A root reached through a symlink (`/tmp` → `/private/tmp` on macOS) is the same root:
    /// a path the harness names through either spelling stays root-relative, with no false
    /// identity added.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_root_spelled_through_a_link_is_the_same_root() {
        let world = tempfile::tempdir().expect("tempdir");
        let real = std::fs::canonicalize(world.path()).expect("real");
        let project = real.join("project");
        std::fs::create_dir_all(project.join("out")).expect("project");
        let alias = real.join("alias");
        std::os::unix::fs::symlink(&project, &alias).expect("alias");
        let got = anchor(
            vec![
                project.join("out/a.md").to_string_lossy().into_owned(),
                alias.join("out/b.md").to_string_lossy().into_owned(),
            ],
            &alias,
        )
        .await;
        assert_eq!(got, ["out/a.md", "out/b.md"]);
    }
}
