// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The rehearsal kit — what the try room gets beyond the fixtures a body
//! names ([`nika_pack::try_rehearsal_kit`] · #1544). Try-only by law: the
//! room is the engine's own temp dir, so the kit's files are written
//! outright (the staged yaml is too); `nika new` takes the fixtures through
//! [`crate::fixtures`] and never plants a demo `VERSION` beside an
//! operator's real repository.

use std::path::Path;

/// Plant the kit's files at the room root. Returns how many were written
/// (`0` for a job without a kit).
///
/// # Errors
/// An I/O failure creating a directory or writing a file — the door
/// reports it and refuses to rehearse on a half-staged room.
pub fn stage(slug: &str, room: &Path) -> std::io::Result<usize> {
    let Some(kit) = nika_pack::try_rehearsal_kit(slug) else {
        return Ok(0);
    };
    for (rel, content) in &kit.files {
        let target = room.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, content)?;
    }
    Ok(kit.files.len())
}

/// The kit's `KEY=VALUE` inputs the operator did NOT name — an operator's
/// own `--var key=…` always wins, and a job without a kit adds nothing.
#[must_use = "the rehearsal inputs must be passed to the run"]
pub fn supplied_vars(slug: &str, operator: &[String]) -> impl Iterator<Item = String> {
    let Some(kit) = nika_pack::try_rehearsal_kit(slug) else {
        return Vec::new().into_iter();
    };
    let named = |key: &str| {
        operator
            .iter()
            .any(|v| v.split_once('=').is_some_and(|(k, _)| k.trim() == key))
    };
    kit.vars
        .iter()
        .filter(|(key, _)| !named(key))
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-rehearsal-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmpdir");
        dir
    }

    /// A job that rehearses on its own gets no kit: the room stays empty
    /// and the `--var` list is untouched.
    #[test]
    fn a_job_without_a_kit_stages_nothing_and_adds_nothing() {
        let dir = room("none");
        assert_eq!(stage("01-hello", &dir).expect("nothing to write"), 0);
        assert!(
            std::fs::read_dir(&dir).expect("room").next().is_none(),
            "the room stays empty"
        );
        assert!(supplied_vars("01-hello", &[]).next().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #1544 · the release train's room carries the files its three gates
    /// read, in the nested dir the schema path names, and a second staging
    /// rewrites them outright (the room is the engine's, never the operator's).
    #[test]
    fn the_release_train_room_carries_its_demo_release() {
        let kit = nika_pack::try_rehearsal_kit("release-train").expect("#1544 kit");
        let dir = room("release-train");
        assert_eq!(stage("release-train", &dir).expect("kit"), kit.files.len());
        for (rel, content) in &kit.files {
            assert_eq!(
                std::fs::read_to_string(dir.join(rel)).expect(rel),
                *content,
                "`{rel}` lands verbatim"
            );
        }
        std::fs::write(dir.join("VERSION"), "stale\n").expect("a stale room");
        assert_eq!(
            stage("release-train", &dir).expect("again"),
            kit.files.len()
        );
        assert_ne!(
            std::fs::read_to_string(dir.join("VERSION")).expect("VERSION"),
            "stale\n",
            "a stale room is rewritten, never kept"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn staging_errors_refuse_a_partial_room() {
        let dir = room("blocked");
        std::fs::write(dir.join("schemas"), "a file blocks the schema directory").expect("block");
        assert!(stage("release-train", &dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn taking_the_real_example_never_plants_a_demo_release() {
        let dir = room("new");
        let body = nika_pack::example("release-train").expect("example");
        crate::fixtures::materialize(body, &dir.join("release.nika.yaml")).expect("take");
        assert!(!dir.join("VERSION").exists());
        assert!(!dir.join("CHANGELOG.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The operator's own `--var` for a kit key wins; another key does not
    /// shadow it; `hello` aliases `01-hello` on the kit lookup too.
    #[test]
    fn the_operators_own_var_wins_over_the_kit() {
        let kit = nika_pack::try_rehearsal_kit("release-train").expect("#1544 kit");
        let (key, value) = kit.vars[0];
        assert_eq!(
            supplied_vars("release-train", &[]).collect::<Vec<_>>(),
            [format!("{key}={value}")]
        );
        assert!(
            supplied_vars("release-train", &[format!("{key}=9.9.9")])
                .next()
                .is_none()
        );
        assert_eq!(
            supplied_vars("release-train", &["hold_for=2s".to_owned()]).collect::<Vec<_>>(),
            [format!("{key}={value}")]
        );
        assert_eq!(supplied_vars("release-train.nika.yaml", &[]).count(), 1);
        assert!(supplied_vars("hello", &[]).next().is_none());
    }
}
