// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The paths a permission ask names, in the workflow's own vocabulary (NIK-266).
//!
//! `permits.fs` globs are authored relative to the project (`./out/**`), while a harness names
//! the paths it will touch absolutely (Codex and Claude Code edit asks carry the full path).
//! The boundary predicate never matches an absolute path against a relative glob (a host path
//! is not a workspace path), so an ask INSIDE the grants read as outside them. Each location
//! is therefore projected onto the session root: inside it, root-relative; outside it,
//! absolute (only an absolute grant can admit it).
//!
//! The harness, not Nika, performs the action, so what is judged is where the operating
//! system will land: the location's EFFECTIVE IDENTITY, walked physically from the raw path
//! the harness named, component by component, through every symlink (a dangling one
//! included) and every `..` as the kernel resolves it — `out/link/../x` with `out/link`
//! pointing outside lands beside the link's TARGET, not in `out/`. When the identity differs
//! from the displayed projection it is added as a location too, and the bridge requires every
//! location to fit (the NEP-0009 identity law, harness side). A walk that cannot complete (a
//! symlink loop, an unreadable component, a component that is not a directory) yields a
//! location no grant can admit, so the ask pauses: never a fallback to the raw text.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use nika_types::access::HARNESS_UNRESOLVED_PATH;

/// The most symlinks one location may traverse (the Linux `MAXSYMLINKS`, the `ELOOP` bound).
const MAX_LINK_HOPS: u32 = 40;

/// Fold `.` and `..` lexically, for the DISPLAYED projection only (`..` never climbs above
/// the root of an absolute path; a relative path keeps its leading escape).
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(component),
            },
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

/// One step of a physical walk.
enum Step {
    Root,
    Parent,
    Name(OsString),
}

/// The steps of `path`, or `None` for a component this walk does not read (a drive prefix).
fn steps(path: &Path) -> Option<Vec<Step>> {
    let mut out = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir => out.push(Step::Root),
            Component::CurDir => {}
            Component::ParentDir => out.push(Step::Parent),
            Component::Normal(name) => out.push(Step::Name(name.to_owned())),
            Component::Prefix(_) => return None,
        }
    }
    Some(out)
}

/// The effective identity of the absolute `raw` path: walked from `/`, the resolved prefix
/// kept canonical. A Normal component is lstat'ed: a symlink is followed (its target resolved
/// against the link's parent, recursively, a missing target included, at most
/// [`MAX_LINK_HOPS`] hops); a missing one starts the missing tail, which later components
/// join. `..` pops the missing tail when there is one, else the canonical prefix. Any other
/// failure is the reason the identity is unresolvable.
fn physical(raw: &Path) -> Result<PathBuf, &'static str> {
    if !raw.is_absolute() {
        return Err("the path is not absolute");
    }
    let mut queue: VecDeque<Step> = steps(raw)
        .ok_or("a path prefix this walk does not read")?
        .into();
    let mut prefix = PathBuf::from("/");
    let mut tail: Vec<OsString> = Vec::new();
    let mut hops = 0_u32;
    while let Some(step) = queue.pop_front() {
        match step {
            Step::Root => {
                prefix = PathBuf::from("/");
                tail.clear();
            }
            Step::Parent => {
                if tail.pop().is_none() {
                    prefix.pop();
                }
            }
            Step::Name(name) if !tail.is_empty() => tail.push(name),
            Step::Name(name) => {
                let candidate = prefix.join(&name);
                match std::fs::symlink_metadata(&candidate) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        hops += 1;
                        if hops > MAX_LINK_HOPS {
                            return Err("a symlink loop");
                        }
                        let target = std::fs::read_link(&candidate)
                            .map_err(|_| "a symlink that cannot be read")?;
                        let target =
                            steps(&target).ok_or("a symlink target this walk does not read")?;
                        if target.is_empty() {
                            return Err("an empty symlink");
                        }
                        for step in target.into_iter().rev() {
                            queue.push_front(step);
                        }
                    }
                    Ok(_) => prefix = candidate,
                    Err(e) if e.kind() == ErrorKind::NotFound => tail.push(name),
                    Err(_) => return Err("a path component that cannot be resolved"),
                }
            }
        }
    }
    for name in tail {
        prefix.push(name);
    }
    Ok(prefix)
}

/// The location naming an identity the walk could not resolve: [`HARNESS_UNRESOLVED_PATH`]
/// first, so no grant can admit it, then the displayed path and the reason.
fn unresolved(projected: &str, why: &str) -> String {
    format!("{HARNESS_UNRESOLVED_PATH} {projected} · {why}")
}

/// [`anchor`]'s filesystem half, run off the async executor.
fn anchor_now(locations: &[String], root: &Path) -> Vec<String> {
    let lexical_root = lexical(root);
    let real_root = std::fs::canonicalize(&lexical_root).ok();
    let roots: Vec<&Path> = std::iter::once(lexical_root.as_path())
        .chain(real_root.as_deref())
        .collect();
    let mut out = Vec::with_capacity(locations.len());
    for named in locations {
        let path = Path::new(named);
        // The RAW path, never folded: `..` is resolved by the walk, as the kernel would.
        let raw = if path.is_absolute() {
            path.to_path_buf()
        } else {
            lexical_root.join(path)
        };
        let projected = project(&lexical(&raw), &roots);
        let identity = match physical(&raw) {
            Ok(real) => project(&real, &roots),
            Err(why) => unresolved(&projected, why),
        };
        let moved = identity != projected;
        out.push(projected);
        if moved {
            out.push(identity);
        }
    }
    out
}

/// Project the wire's `locations` onto the session `root` (see the module doc): each named
/// path, root-relative inside the root, then its effective identity when that differs.
pub(super) async fn anchor(locations: Vec<String>, root: &Path) -> Vec<String> {
    if locations.is_empty() {
        return locations;
    }
    let root = root.to_path_buf();
    let named = locations.clone();
    match tokio::task::spawn_blocking(move || anchor_now(&locations, &root)).await {
        Ok(anchored) => anchored,
        // The walk never answered: no identity is resolved, every location pauses.
        Err(_) => named
            .into_iter()
            .flat_map(|named| {
                let marker = unresolved(&named, "the walk did not complete");
                [named, marker]
            })
            .collect(),
    }
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

    /// A disposable world: a canonical `project/out` and a sibling `outside/deep`.
    #[cfg(unix)]
    struct World {
        _dir: tempfile::TempDir,
        project: PathBuf,
        outside: PathBuf,
    }

    #[cfg(unix)]
    fn world() -> World {
        let dir = tempfile::tempdir().expect("tempdir");
        let base = std::fs::canonicalize(dir.path()).expect("real base");
        let (project, outside) = (base.join("project"), base.join("outside"));
        std::fs::create_dir_all(project.join("out")).expect("project");
        std::fs::create_dir_all(outside.join("deep")).expect("outside");
        World {
            _dir: dir,
            project,
            outside,
        }
    }

    #[cfg(unix)]
    fn link(target: &Path, at: &Path) {
        std::os::unix::fs::symlink(target, at).expect("link");
    }

    #[cfg(unix)]
    fn text(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    /// The review's counterexample (NIK-266): `out/link/../escaped.md` with `out/link` pointing
    /// at `outside/deep`. The display folds it to `out/escaped.md`, but the kernel resolves
    /// `link/..` to the TARGET's parent, so the write lands at `outside/escaped.md`: that
    /// identity is what is judged.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_parent_step_through_a_link_lands_beside_the_links_target() {
        let w = world();
        link(&w.outside.join("deep"), &w.project.join("out/link"));
        let named = text(&w.project.join("out/link/../escaped.md"));
        let got = anchor(vec![named], &w.project).await;
        assert_eq!(
            got,
            [
                "out/escaped.md".to_owned(),
                text(&w.outside.join("escaped.md"))
            ]
        );
    }

    /// (a) A dangling link inside the project: its target does not exist, yet writing through
    /// it creates the target outside. The walk follows it anyway.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_dangling_link_is_judged_where_it_would_create_its_target() {
        let w = world();
        link(&w.outside.join("new"), &w.project.join("out/d"));
        let got = anchor(
            vec![text(&w.project.join("out/d")), "out/d/x.md".to_owned()],
            &w.project,
        )
        .await;
        assert_eq!(
            got,
            [
                "out/d".to_owned(),
                text(&w.outside.join("new")),
                "out/d/x.md".to_owned(),
                text(&w.outside.join("new/x.md")),
            ]
        );
    }

    /// (b) `out/link/missing/../../x`: `missing/..` pops the missing tail, the second `..`
    /// pops the link target's real parent. No location ever carries a `..`.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_missing_component_then_parents_resolve_physically() {
        let w = world();
        link(&w.outside.join("deep"), &w.project.join("out/link"));
        let got = anchor(vec!["out/link/missing/../../x".to_owned()], &w.project).await;
        assert_eq!(got, ["out/x".to_owned(), text(&w.outside.join("x"))]);
        assert!(
            got.iter().all(|location| !location.contains("..")),
            "{got:?}"
        );
    }

    /// A chain of links (relative, then absolute) is followed to its end; a chain that stays
    /// inside the project stays root-relative.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_link_chain_is_followed_to_its_end() {
        let w = world();
        link(Path::new("b"), &w.project.join("out/a"));
        link(&w.outside.join("deep"), &w.project.join("out/b"));
        std::fs::create_dir_all(w.project.join("src")).expect("src");
        link(Path::new("../src"), &w.project.join("out/inner"));
        let got = anchor(
            vec!["out/a/f.md".to_owned(), "out/inner/lib.rs".to_owned()],
            &w.project,
        )
        .await;
        assert_eq!(
            got,
            [
                "out/a/f.md".to_owned(),
                text(&w.outside.join("deep/f.md")),
                "out/inner/lib.rs".to_owned(),
                "src/lib.rs".to_owned(),
            ]
        );
    }

    /// A symlink loop cannot be resolved: the location fails CLOSED, a marker no grant admits,
    /// never the raw text.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_link_loop_fails_closed() {
        let w = world();
        link(Path::new("l2"), &w.project.join("out/l1"));
        link(Path::new("l1"), &w.project.join("out/l2"));
        let got = anchor(vec!["out/l1/f.md".to_owned()], &w.project).await;
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[0], "out/l1/f.md");
        assert_eq!(
            got[1],
            format!("{HARNESS_UNRESOLVED_PATH} out/l1/f.md · a symlink loop")
        );
    }
}
