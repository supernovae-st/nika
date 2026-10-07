// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trial inputs of a remote compile: the text of the stated inputs the observer read, so a
//! door that holds no project (Serve) tries a candidate in the same observed room the CLI and the
//! Session use, over a scratch project made of exactly these files. [`inputs`] reads them on the
//! caller's host for `nika compile --observe-only` (UTF-8 text, at most the room's copy bound in
//! all, else none, never a cut); `crate::compile::remote::admit_trial` is the door's law;
//! [`TrialProject`] is the scratch project it builds. The seat never reads these bytes, only the
//! trial's report.

use std::path::Path;

use nika_compile_seats::remote::{TRIAL_BYTES, observed_paths, relative};
use serde_json::{Value, json};

/// The bytes of every input `world` marks `observed`, read under `root` (`{"files": [{path,
/// text}]}`), or `None` when there is none, one is not UTF-8 text, or they exceed the bound.
#[must_use]
pub fn inputs(root: &Path, world: &Value) -> Option<Value> {
    let real_root = root.canonicalize().ok()?;
    let mut total = 0_u64;
    let mut files = Vec::new();
    for path in observed_paths(world) {
        let relative = relative(&path)?;
        let real = real_root.join(relative).canonicalize().ok()?;
        if !real.starts_with(&real_root) || !real.is_file() {
            return None;
        }
        total = total.checked_add(std::fs::metadata(&real).ok()?.len())?;
        if total > TRIAL_BYTES {
            return None;
        }
        let text = std::fs::read_to_string(&real).ok()?;
        files.push(json!({"path": path, "text": text}));
    }
    (!files.is_empty()).then(|| json!({ "files": files }))
}

/// Write admitted trial inputs into `root` (a fresh directory the door owns) at their relative
/// paths.
///
/// # Errors
/// A path that leaves the root, or a filesystem failure.
pub fn materialize(root: &Path, trial: &Value) -> std::io::Result<()> {
    let refuse = || std::io::Error::new(std::io::ErrorKind::InvalidInput, "trial input path");
    let files = trial["files"].as_array().ok_or_else(refuse)?;
    for file in files {
        let path = file["path"].as_str().ok_or_else(refuse)?;
        let text = file["text"].as_str().ok_or_else(refuse)?;
        let target = root.join(relative(path).ok_or_else(refuse)?);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, text)?;
    }
    Ok(())
}

/// A scratch project made of admitted trial inputs: removed when dropped.
#[derive(Debug)]
pub struct TrialProject(tempfile::TempDir);

impl TrialProject {
    /// A fresh project under the system's temporary directory holding exactly `trial`'s files.
    ///
    /// # Errors
    /// A path that leaves the root, or a filesystem failure.
    pub fn new(trial: &Value) -> std::io::Result<Self> {
        let project = tempfile::tempdir()?;
        materialize(project.path(), trial)?;
        Ok(Self(project))
    }

    /// A project of `trial`'s files and the observed room over it, `jq` evaluating its steps.
    ///
    /// # Errors
    /// As [`Self::new`].
    pub fn room(
        trial: &Value,
        jq: &crate::compile::room::JqHelper,
    ) -> std::io::Result<(Self, crate::compile::room::ObservedRoom)> {
        let project = Self::new(trial)?;
        let room =
            crate::compile::room::ObservedRoom::new(project.path()).with_jq_helper(jq.clone());
        Ok((project, room))
    }

    /// The project root.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.0.path()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_observed_inputs_round_trip_into_a_trial_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let rows = r#"[{"id":"a","status":"open"},{"id":"b","status":"done"}]"#;
        std::fs::write(dir.path().join("data/t.json"), rows).unwrap();
        let intent = "Read ./data/t.json and ./missing.json, save ./out/x.json";
        let world = json!({"observed": [{"path": "./data/t.json", "state": "observed"},
            {"path": "./missing.json", "state": "absent"}]});
        assert_eq!(
            crate::compile::remote::admit_observation(intent, &world),
            Ok(())
        );
        let trial = inputs(dir.path(), &world).expect("the observed file");
        assert_eq!(
            trial,
            json!({"files": [{"path": "./data/t.json", "text": rows}]})
        );
        assert_eq!(crate::compile::remote::admit_trial(&world, &trial), Ok(()));
        let project = TrialProject::new(&trial).unwrap();
        let copied = std::fs::read_to_string(project.path().join("data/t.json")).unwrap();
        assert_eq!(copied, rows);
        assert_eq!(TRIAL_BYTES, crate::compile::room::ObservedRoom::COPY_BOUND);
    }

    #[test]
    fn a_world_past_the_bound_or_not_text_sends_no_trial_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let bound = usize::try_from(TRIAL_BYTES).unwrap();
        std::fs::write(dir.path().join("big.csv"), "a\n".repeat(bound)).unwrap();
        let world = json!({"observed": [{"path": "./big.csv", "state": "observed"}]});
        assert_eq!(inputs(dir.path(), &world), None);
        std::fs::write(dir.path().join("bin.csv"), [0xff_u8, 0xfe, 0x00]).unwrap();
        let world = json!({"observed": [{"path": "./bin.csv", "state": "observed"}]});
        assert_eq!(inputs(dir.path(), &world), None);
    }
}
