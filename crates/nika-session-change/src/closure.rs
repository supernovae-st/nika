// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The world a Session check judges and a run admits, read once by the execution reader: the
//! workflow, every child workflow it reaches and every skill, as `nika run` captures them.

use std::path::{Component, Path, PathBuf};

use nika_execution::{ExecutionSnapshot, SnapshotLimits};
use nika_fs::OwnedDir;

/// The world a Session check judged: the execution reader's snapshot digest (SHA-256 over format,
/// root, unit roles, paths and bytes) of the workflow, every child workflow it reaches and every
/// skill. A door runs only a world with the same digest. It serializes as the hex digest.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct Closure(pub String);

impl Closure {
    /// The closure of a captured world.
    #[must_use]
    pub fn of(world: &ExecutionSnapshot) -> Self {
        Self(world.digest().to_owned())
    }

    /// Whether `world` is exactly the world this closure names.
    #[must_use]
    pub fn admits(&self, world: &ExecutionSnapshot) -> bool {
        self.0 == world.digest()
    }
}

/// The world of `path` under `root`, as a run captures it.
pub(crate) fn capture(root: &Path, path: &Path) -> Result<ExecutionSnapshot, String> {
    let project = OwnedDir::open(root).map_err(|e| e.to_string())?;
    ExecutionSnapshot::capture(&project, path, SnapshotLimits::default()).map_err(|e| e.to_string())
}

/// The bytes the check reads at `read` (a path it resolved lexically from the workflow's own),
/// served from the captured world, never from the disk a second time.
pub(crate) fn served(world: &ExecutionSnapshot, root: &Path, read: &str) -> Result<String, String> {
    let logical = lexical(Path::new(read))
        .strip_prefix(lexical(root))
        .ok()
        .map(|relative| {
            let parts: Vec<_> = (relative.components())
                .map(|part| part.as_os_str().to_string_lossy())
                .collect();
            parts.join("/")
        });
    (logical.as_deref())
        .and_then(|logical| world.text(logical))
        .map(str::to_owned)
        .ok_or_else(|| format!("`{read}` is not in the world this check captured"))
}

/// The closure a check judged: the world captured before it, when the world captured after it is
/// the same. Otherwise none, and a check that found nothing else says why the run cannot hold it.
pub(crate) fn settled(
    before: Result<ExecutionSnapshot, String>,
    root: &Path,
    path: &Path,
) -> Result<Closure, String> {
    let before = before.map_err(|e| format!("the run cannot capture this workflow: {e}"))?;
    match capture(root, path) {
        Ok(after) if after.digest() == before.digest() => Ok(Closure::of(&before)),
        Ok(_) => Err("the workflow or a workflow it calls changed while it was checked".into()),
        Err(e) => Err(format!("the run cannot capture this workflow: {e}")),
    }
}

/// `.` and `..` resolved lexically, the way the check resolves a child's path.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::change::{RunRequest, check_on_disk};

    const PARENT: &str =
        "nika: parent\ntasks:\n  call:\n    invoke: { workflow: \"./child.nika\" }\n";
    const CHILD: &str = "nika: kid\nmodel: mock/echo\ntasks:\n  echo:\n    infer: { prompt: child-ran, max_tokens: 20 }\noutputs:\n  said: ${{ tasks.echo.output }}\n";

    /// The check judges the world a run captures, read once: its closure is that capture's, a
    /// request bound to it admits that world, and a child rewritten since (still valid, the parent
    /// unchanged) is another world, refused.
    #[test]
    fn a_check_names_the_world_it_judged_and_a_rewritten_child_is_another() {
        let dir = tempfile::tempdir().expect("project");
        std::fs::write(dir.path().join("parent.nika"), PARENT).expect("parent");
        std::fs::write(dir.path().join("child.nika"), CHILD).expect("child");
        let audit = check_on_disk(dir.path(), Path::new("parent.nika"));
        assert!(audit.clean, "{:?}", audit.findings);
        let world = super::capture(dir.path(), Path::new("parent.nika")).expect("world");
        assert_eq!(audit.closure, Some(super::Closure::of(&world)));
        assert!(
            world
                .units()
                .any(|unit| unit.logical_path() == "child.nika"),
            "the child is in the world"
        );
        let run = RunRequest {
            workflow: PathBuf::from("parent.nika"),
            vars: Vec::new(),
            max_cost_usd: 0.1,
            access_pin: None,
            bytes: audit.bytes.clone().map(Box::new),
            closure: audit.closure.clone().map(Box::new),
        };
        assert!(run.admits_world(&world));
        let child = CHILD.replace("child-ran", "rewritten");
        std::fs::write(dir.path().join("child.nika"), child).expect("child rewritten");
        let moved = super::capture(dir.path(), Path::new("parent.nika")).expect("world");
        assert!(run.admits(PARENT), "the parent's own bytes did not change");
        assert!(
            !run.admits_world(&moved),
            "a rewritten child is another world"
        );
        let unbound = RunRequest {
            closure: None,
            ..run
        };
        assert!(
            !unbound.admits_world(&world),
            "no recorded closure admits none"
        );
    }

    /// A path the check resolves outside the captured world is never read from the disk.
    #[test]
    fn a_read_outside_the_captured_world_is_refused() {
        let dir = tempfile::tempdir().expect("project");
        std::fs::write(dir.path().join("parent.nika"), PARENT).expect("parent");
        std::fs::write(dir.path().join("child.nika"), CHILD).expect("child");
        let world = super::capture(dir.path(), Path::new("parent.nika")).expect("world");
        let inside = dir.path().join("sub/../child.nika");
        let served = super::served(&world, dir.path(), &inside.display().to_string());
        assert_eq!(served.as_deref(), Ok(CHILD));
        let outside = dir.path().join("../elsewhere.nika");
        let refused = super::served(&world, dir.path(), &outside.display().to_string());
        assert!(refused.is_err(), "{refused:?}");
    }
}
