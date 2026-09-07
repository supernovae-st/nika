// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ancestor `nika.yaml` `ceiling:` on `nika check`.
//!
//! `nika run` already walks up from CWD and fills `--max-cost-usd` from
//! that file (the ladder in `run::ceiling`). `nika check` priced the
//! workflow in isolation and printed « no total ceiling » / « cap it on
//! the run with --max-cost-usd » while the same tree already carried a
//! spend cap. The value is recorded in the post-run trace; this module
//! puts the same number on the pre-run surface, with its provenance
//! (#1050).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nika_vocab::project::ProjectError;

use crate::display::theme::{Role, Theme};

/// A project-file ceiling that `nika run` from this CWD would honour.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct AmbientCeiling {
    pub usd: f64,
    pub path: PathBuf,
    pub line: Option<usize>,
}

/// Walk from `start` the same way `run::ceiling::ladder` does.
pub(super) fn at(start: &Path) -> Result<Option<AmbientCeiling>, ProjectError> {
    let Some((path, project)) = nika_vocab::project::discover(start)? else {
        return Ok(None);
    };
    let Some(usd) = project.ceiling else {
        return Ok(None);
    };
    let line = ceiling_line(&path);
    Ok(Some(AmbientCeiling { usd, path, line }))
}

/// The CWD door — identical start to `nika run` with no flag.
pub(super) fn from_cwd() -> Result<Option<AmbientCeiling>, ProjectError> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    at(&cwd)
}

/// Human footnote. Presence-gated: silence when no file governs spend.
pub(super) fn footnote(text: &mut String, theme: Theme, ceiling: Option<&AmbientCeiling>) {
    let Some(c) = ceiling else {
        return;
    };
    let where_ = provenance(c);
    let _ = writeln!(
        text,
        " {} {}   ${} ← {where_} · the run's spend cap when `--max-cost-usd` is omitted",
        theme.paint(Role::Accent, "↳"),
        theme.paint(Role::Strong, "BUDGET"),
        nika_display::vocab::usd(c.usd),
    );
}

/// Presence-gated `--json` object. `clean` does not read it.
pub(super) fn stamp_json(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    ceiling: Option<&AmbientCeiling>,
) {
    let Some(c) = ceiling else {
        return;
    };
    let mut v = serde_json::json!({
        "max_cost_usd": c.usd,
        "source": c.path.display().to_string(),
        "via": "project",
    });
    if let Some(line) = c.line
        && let Some(o) = v.as_object_mut()
    {
        o.insert("line".to_owned(), serde_json::json!(line));
    }
    obj.insert("run_budget".to_owned(), v);
}

fn provenance(c: &AmbientCeiling) -> String {
    let name = c
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("nika.yaml");
    match c.line {
        Some(line) => format!("{name}:{line}"),
        None => name.to_owned(),
    }
}

/// 1-based line of the `ceiling:` key. Best-effort: the parser keeps
/// spans only on refusals, so a green file is re-read here.
fn ceiling_line(path: &Path) -> Option<usize> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().enumerate().find_map(|(i, raw)| {
        let t = raw.trim_start();
        t.starts_with("ceiling:").then_some(i + 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // `set_current_dir` is process-global — so the lease is the CRATE's, not
    // this module's (#1192). The private `CWD_LOCK` that stood here excluded
    // only the tests below: `arm fire` took a different mutex and
    // `run --example` took none, so a test could hold this lock for its whole
    // body and still have the ground moved under it. Measured: these three
    // tests failed one CI run in four.

    fn fresh(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nika-check-budget-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[test]
    fn an_ancestor_ceiling_is_the_ambient_budget() {
        let root = fresh("ancestor");
        let child = root.join("sub");
        std::fs::create_dir_all(&child).expect("mkdir");
        std::fs::write(root.join("nika.yaml"), "nika: proj\nceiling: 0.01\n").expect("seed");
        let c = at(&child).expect("valid project").expect("walks up");
        assert_eq!(c.usd.to_bits(), 0.01f64.to_bits());
        assert_eq!(c.line, Some(2));
        assert!(c.path.ends_with("nika.yaml"), "{:?}", c.path);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_without_ceiling_is_silence() {
        let dir = fresh("none");
        std::fs::write(dir.join("nika.yaml"), "nika: proj\n").expect("seed");
        assert!(
            at(&dir).expect("valid project").is_none(),
            "no ceiling → check stays silent"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_boundary_file_stops_the_walk() {
        // Same shape as the engine's own nika.yaml: a name and nothing
        // else, so a tempdir nested under /tmp cannot pick up a stray
        // ancestor ceiling.
        let dir = fresh("boundary");
        std::fs::write(dir.join("nika.yaml"), "nika: boundary\n").expect("seed");
        assert!(at(&dir).expect("valid boundary").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_nearest_file_wins() {
        let root = fresh("nearest");
        let child = root.join("sub");
        std::fs::create_dir_all(&child).expect("mkdir");
        std::fs::write(root.join("nika.yaml"), "nika: root\nceiling: 9.99\n").expect("root");
        std::fs::write(child.join("nika.yaml"), "nika: leaf\nceiling: 0.25\n").expect("leaf");
        let c = at(&child).expect("valid project").expect("leaf");
        assert_eq!(
            c.usd.to_bits(),
            0.25f64.to_bits(),
            "the first file on the walk governs"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_invalid_nearest_project_preserves_its_error_instead_of_inheriting() {
        let room = tempfile::tempdir().expect("room");
        let child = room.path().join("child");
        std::fs::create_dir(&child).expect("child");
        std::fs::write(room.path().join("nika.yaml"), "nika: root\nceiling: 0.50\n")
            .expect("ancestor");
        let path = child.join("nika.yaml");
        for yaml in [
            "nika: child\nceiling: 0\n",
            "nika: child\nceiling: -1\n",
            "nika: child\nceiling: \"0.01\"\n",
            "nika: child\nceiling: [\n",
        ] {
            std::fs::write(&path, yaml).expect("invalid nearest project");
            let expected = nika_vocab::project::parse(yaml).expect_err("parser refusal");
            let err = at(&child).expect_err("present invalid project is not absence");
            assert_eq!(err.path(), Some(path.as_path()));
            assert_eq!(err.kind(), expected.kind());
            assert_eq!(err.line(), expected.line());
            assert_eq!(err.detail(), expected.detail());
            assert_eq!(err.remedy(), expected.remedy());
        }
    }

    #[test]
    fn an_unreadable_nearest_project_is_not_absence() {
        let room = tempfile::tempdir().expect("room");
        let path = room.path().join("nika.yaml");
        std::fs::create_dir(&path).expect("directory in place of project");
        let err = at(room.path()).expect_err("unreadable project");
        assert_eq!(
            err.kind(),
            nika_vocab::project::ProjectErrorKind::Unreadable
        );
        assert_eq!(err.path(), Some(path.as_path()));
        assert_eq!(err.line(), None);
    }

    #[test]
    fn a_valid_bare_project_does_not_inherit_an_ancestor_ceiling() {
        let room = tempfile::tempdir().expect("room");
        let child = room.path().join("child");
        std::fs::create_dir(&child).expect("child");
        std::fs::write(room.path().join("nika.yaml"), "nika: root\nceiling: 0.50\n")
            .expect("ancestor");
        std::fs::write(child.join("nika.yaml"), "nika: child\n").expect("boundary");
        assert!(at(&child).expect("valid boundary").is_none());
    }

    #[test]
    fn footnote_names_the_file_and_the_dollars() {
        let _lock = crate::cwd::hold();
        let dir = fresh("footnote");
        std::fs::write(dir.join("nika.yaml"), "nika: proj\nceiling: 0.01\n").expect("seed");
        let prev = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&dir).expect("chdir");
        let mut text = String::new();
        let ceiling = from_cwd().expect("valid project");
        footnote(&mut text, Theme::new(false, true, false), ceiling.as_ref());
        let _ = std::env::set_current_dir(prev);
        assert!(
            text.contains("BUDGET") && text.contains("0.0100") && text.contains("nika.yaml:2"),
            "{text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_run_names_the_ancestor_ceiling() {
        let _lock = crate::cwd::hold();
        let dir = fresh("run");
        std::fs::write(dir.join("nika.yaml"), "nika: proj\nceiling: 0.01\n").expect("seed");
        std::fs::write(
            dir.join("wf.nika.yaml"),
            "nika: wf\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n",
        )
        .expect("wf");
        let prev = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&dir).expect("chdir");
        let out = crate::verbs::check::run(
            "wf.nika.yaml",
            false,
            false,
            None,
            Theme::new(false, true, false),
        );
        let json = crate::verbs::check::run(
            "wf.nika.yaml",
            true,
            false,
            None,
            Theme::new(false, true, false),
        );
        let _ = std::env::set_current_dir(&prev);
        assert!(
            out.text.contains("BUDGET")
                && out.text.contains("0.0100")
                && out.text.contains("nika.yaml:2"),
            "{}",
            out.text
        );
        let v: serde_json::Value = serde_json::from_str(&json.text).expect("json");
        assert_eq!(v["run_budget"]["max_cost_usd"], 0.01, "{v}");
        assert_eq!(v["run_budget"]["line"], 2, "{v}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_is_presence_gated_and_carries_provenance() {
        let _lock = crate::cwd::hold();
        let dir = fresh("json");
        std::fs::write(dir.join("nika.yaml"), "nika: proj\nceiling: 0.01\n").expect("seed");
        let prev = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&dir).expect("chdir");
        let mut obj = serde_json::Map::new();
        let ceiling = from_cwd().expect("valid project");
        stamp_json(&mut obj, ceiling.as_ref());
        let _ = std::env::set_current_dir(&prev);
        let v = obj.get("run_budget").expect("present");
        assert_eq!(v["max_cost_usd"], 0.01);
        assert_eq!(v["via"], "project");
        assert_eq!(v["line"], 2);
        assert!(
            v["source"]
                .as_str()
                .is_some_and(|s| s.ends_with("nika.yaml")),
            "{v}"
        );
        let empty = fresh("json-empty");
        std::fs::write(empty.join("nika.yaml"), "nika: boundary\n").expect("boundary");
        std::env::set_current_dir(&empty).expect("chdir empty");
        let mut silent = serde_json::Map::new();
        let ceiling = from_cwd().expect("valid boundary");
        stamp_json(&mut silent, ceiling.as_ref());
        let _ = std::env::set_current_dir(prev);
        assert!(silent.is_empty(), "{silent:?}");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }
}
