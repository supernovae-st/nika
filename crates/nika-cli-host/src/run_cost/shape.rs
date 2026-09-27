// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Fresh descriptor-rooted observations; static shape analysis lives in L3.
use nika_check::analyzer::static_args::ConstStrings;
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_service_execution::run_cost::project_file_path;
use std::io::Read as _;
use std::path::Path;

/// Bind pre-existing read bytes and re-observe contained output parents.
/// No input bytes or callable authority are persisted here.
/// Descriptor-rooted no-follow opens supplement (never replace) Check/permits.
pub(super) fn read_witness(root: &Path, wf: &RawWorkflow) -> Result<String, String> {
    let launch = std::env::current_dir().map_err(|e| e.to_string())?;
    if nika_runtime::project_root_fingerprint(root).ok_or("project root is unreadable")?
        != nika_runtime::project_root_fingerprint(&launch).ok_or("launch root is unreadable")?
    {
        return Err("unknown-cost Run local paths require the launch project root".into());
    }
    let directory = nika_fs::OwnedDir::open(root).map_err(|e| e.to_string())?;
    let consts = ConstStrings::of(wf);
    let mut files = std::collections::BTreeMap::new();
    for task in &wf.tasks {
        if let RawAction::Invoke(action) = &task.value.action
            && action.tool().is_some_and(|t| t.value == "nika:write")
        {
            let path = project_file_path(&consts, action).map_err(|e| e.to_string())?;
            let parts = path
                .iter()
                .map(|s| s.to_str().ok_or("non-UTF-8 path"))
                .collect::<Result<Vec<_>, _>>()?;
            let (name, parents) = parts.split_last().ok_or("empty write path")?;
            if let Some(parent) = write_parent(&directory, parents, creates_dirs(action))? {
                match parent.open_relative(Path::new(name)) {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        return Err(format!("write target is not a contained regular file: {e}"));
                    }
                }
            }
        }
        if let RawAction::Invoke(action) = &task.value.action
            && action.tool().is_some_and(|t| t.value == "nika:read")
        {
            let path = project_file_path(&consts, action).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            directory
                .open_relative(&path)
                .map_err(|e| e.to_string())?
                .take(1_048_577)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 1_048_576 {
                return Err("unknown-cost Run read input exceeds the 1 MiB review bound".into());
            }
            files.insert(path, nika_event::source_id::sha256_hex(&bytes));
        }
    }
    Ok(nika_event::source_id::sha256_hex(
        format!("{files:?}").as_bytes(),
    ))
}

/// Whether the write itself declares `create_dirs: true` as a literal; an
/// absent, false or templated value never lets a missing parent through.
fn creates_dirs(action: &nika_schema::raw::RawInvokeAction) -> bool {
    action
        .args
        .as_ref()
        .and_then(|args| args.value.get("create_dirs"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

/// The write target's contained parent, walked one component at a time and
/// never created here: an existing component must be a real directory (a
/// symlink or a file refuses); a missing one is accepted only when the write
/// itself declares `create_dirs: true`, and then nothing below it exists yet to
/// re-observe (`None`). The review runs this again after the answer, so a
/// component that changes kind meanwhile refuses then.
fn write_parent(
    directory: &nika_fs::OwnedDir,
    parents: &[&str],
    creates: bool,
) -> Result<Option<nika_fs::OwnedDir>, String> {
    let mut current = directory.try_clone().map_err(|e| e.to_string())?;
    for (depth, component) in parents.iter().enumerate() {
        let named = || parents[..=depth].join("/");
        match current.open_below(&[component]) {
            Ok(next) => current = next,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && creates => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(format!(
                    "write target parent `{}` does not exist and the write does not create it (`create_dirs: true` would)",
                    named()
                ));
            }
            Err(e) => {
                return Err(format!(
                    "write target parent `{}` is not a contained directory: {e}",
                    named()
                ));
            }
        }
    }
    Ok(Some(current))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    /// Only a literal `create_dirs: true` on the write lets a missing parent
    /// through; false, absent and templated values never do.
    #[test]
    fn only_a_literal_create_dirs_true_counts() {
        let wf = nika_schema::parse(
            "nika: t\npermits:\n  tools: [nika:write]\n  fs: { write: [./out/**] }\nconst:\n  mk: true\ntasks:\n  a:\n    invoke: { tool: nika:write, args: { path: ./out/a.txt, content: x, create_dirs: true } }\n  b:\n    invoke: { tool: nika:write, args: { path: ./out/b.txt, content: x, create_dirs: false } }\n  c:\n    invoke: { tool: nika:write, args: { path: ./out/c.txt, content: x } }\n  d:\n    invoke: { tool: nika:write, args: { path: ./out/d.txt, content: x, create_dirs: \"${{ const.mk }}\" } }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let declared: Vec<bool> = wf
            .tasks
            .iter()
            .map(|task| match &task.value.action {
                RawAction::Invoke(action) => creates_dirs(action),
                _ => panic!("invoke fixtures only"),
            })
            .collect();
        assert_eq!(declared, [true, false, false, false]);
    }

    fn project() -> (tempfile::TempDir, nika_fs::OwnedDir) {
        let root = tempfile::tempdir().unwrap();
        let dir = nika_fs::OwnedDir::open(root.path()).unwrap();
        (root, dir)
    }

    /// P6 · a missing output parent under `create_dirs: true` reaches the
    /// review (no bare ENOENT) and nothing is created before the answer.
    #[test]
    fn a_missing_parent_the_write_creates_is_admitted_and_never_made_here() {
        let (root, dir) = project();
        assert!(write_parent(&dir, &["reports"], true).unwrap().is_none());
        std::fs::create_dir(root.path().join("out")).unwrap();
        assert!(
            write_parent(&dir, &["out", "deep", "er"], true)
                .unwrap()
                .is_none()
        );
        assert!(!root.path().join("reports").exists(), "no pre-review mkdir");
        assert!(!root.path().join("out").join("deep").exists());
        assert!(
            write_parent(&dir, &["out"], false).unwrap().is_some(),
            "an existing parent is re-observed as before"
        );
        assert!(write_parent(&dir, &[], false).unwrap().is_some());
    }

    #[test]
    fn a_missing_parent_without_create_dirs_is_named_not_a_bare_enoent() {
        let (_root, dir) = project();
        let refused = write_parent(&dir, &["reports", "q3"], false).unwrap_err();
        assert!(refused.contains("`reports` does not exist"), "{refused}");
        assert!(refused.contains("create_dirs: true"), "{refused}");
    }

    /// Whatever the write declares, an existing parent that is not a real
    /// contained directory refuses: a symlink is never followed, a file never
    /// becomes a directory.
    #[test]
    fn a_symlinked_or_file_parent_refuses_even_with_create_dirs() {
        let (root, dir) = project();
        let elsewhere = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), root.path().join("linked")).unwrap();
        std::fs::write(root.path().join("plain"), "x").unwrap();
        for (parents, creates) in [
            (&["linked"][..], true),
            (&["linked", "below"][..], true),
            (&["plain"][..], true),
            (&["plain", "below"][..], false),
        ] {
            let refused = write_parent(&dir, parents, creates).unwrap_err();
            assert!(
                refused.contains("is not a contained directory"),
                "{refused}"
            );
        }
        assert!(!elsewhere.path().join("below").exists());
    }
}
