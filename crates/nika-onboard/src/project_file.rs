// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The starter `nika.yaml` — laid ONLY on an explicit yes
//! (D-2026-08-11-N5). Never silently: the wizard asks (Enter skips,
//! an offer never a toll), the scripted lane has exactly one door
//! (`--project-file`), and the file itself says so — its examples
//! ride commented, so a laid starter governs NOTHING until the human
//! uncomments it (absence IS the defaults, ratchet-pinned in
//! [`nika_vocab::project`]).
//!
//! The adds-only law is the `.gitignore` row's sibling: an existing
//! file is SKIPPED (`--force` overrides), the write creates parents
//! rather than fail on the missing, and the row rides the founding
//! report in both lanes (one law, two doors).

use std::path::Path;

use nika_vocab::project::{FILE_NAME, STARTER};

/// What one `ensure` came to — the founding register's vocabulary
/// (the [`crate`] `gitignore` precedent, project-file side).
#[non_exhaustive]
pub enum Outcome {
    /// The file was absent — created carrying the starter.
    Created,
    /// The file existed — nothing written (the skip law; `--force`
    /// is the only override).
    Skipped,
    /// A write failure — init's one environment error (exit 3).
    Failed(String),
}

/// Lay `dir/nika.yaml` when absent — the explicit-yes write. `force`
/// overwrites (the founding law's one override); every other path is
/// adds-only.
#[must_use]
pub fn ensure(dir: &str, force: bool) -> (String, Outcome) {
    let path = Path::new(dir)
        .join(FILE_NAME)
        .to_string_lossy()
        .into_owned();
    match ensure_inner(&path, force) {
        Ok(outcome) => (path, outcome),
        Err(e) => (path, Outcome::Failed(e.to_string())),
    }
}

/// The io half — `?` propagation, the caller owns the error voice.
fn ensure_inner(path: &str, force: bool) -> std::io::Result<Outcome> {
    if !force && Path::new(path).exists() {
        return Ok(Outcome::Skipped);
    }
    // Same care as the briefs' write: the dir may not exist yet on a
    // bare `init new-dir` — create parents, never fail on the missing.
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, STARTER)?;
    Ok(Outcome::Created)
}

/// The report row for one outcome — the founding register's ✔/·/✖
/// vocabulary, `path` caller-shaped (joined in the scripted lane ·
/// project-relative in the wizard).
#[must_use]
pub fn report(path: &str, outcome: &Outcome) -> (char, String) {
    match outcome {
        Outcome::Created => ('✔', format!("created {path}")),
        Outcome::Skipped => ('·', format!("skipped {path} (exists · --force)")),
        Outcome::Failed(e) => ('✖', format!("{path}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-projfile-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    /// The skip/overwrite law, pinned: absent → created with the
    /// starter · existing → skipped, byte-untouched · `--force` →
    /// overwritten with the starter.
    #[test]
    fn the_skip_overwrite_law() {
        let dir = fresh_dir("law");
        let d = dir.to_str().expect("utf8");

        let (path, outcome) = ensure(d, false);
        assert!(matches!(outcome, Outcome::Created));
        assert!(path.ends_with("nika.yaml"), "{path}");
        let laid = std::fs::read_to_string(dir.join("nika.yaml")).expect("written");
        assert_eq!(laid, STARTER, "the grammar's own starter, verbatim");

        // A hand-written file is never touched without --force.
        std::fs::write(dir.join("nika.yaml"), "nika: proj\nceiling: 9.99\n").expect("seed");
        let (_, outcome) = ensure(d, false);
        assert!(matches!(outcome, Outcome::Skipped), "existing = skip");
        assert_eq!(
            std::fs::read_to_string(dir.join("nika.yaml")).expect("read"),
            "nika: proj\nceiling: 9.99\n",
            "the human's bytes survive"
        );

        let (_, outcome) = ensure(d, true);
        assert!(matches!(outcome, Outcome::Created), "--force overrides");
        assert_eq!(
            std::fs::read_to_string(dir.join("nika.yaml")).expect("read"),
            STARTER
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The laid starter PARSES — and to the defaults (the offer can
    /// never lay a broken or a silently-governing file).
    #[test]
    fn the_laid_starter_parses_to_the_defaults() {
        let dir = fresh_dir("parses");
        let d = dir.to_str().expect("utf8");
        let _ = ensure(d, false);
        let parsed = nika_vocab::project::discover(&dir)
            .expect("the laid file parses")
            .map(|(_path, project)| project);
        let parsed = parsed.expect("the laid file is discovered");
        assert_eq!(
            parsed.name.as_deref(),
            Some("my-project"),
            "the starter lays a NAME — the version rides the $schema line"
        );
        // `Project` is `#[non_exhaustive]` (FCI-016), so a sibling crate
        // compares the knobs rather than rebuilding the struct.
        assert!(
            parsed.ceiling.is_none()
                && parsed.traces.is_none()
                && parsed.registry.is_none()
                && parsed.arm().is_empty(),
            "every other starter line is a commented example: {parsed:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The report rows speak the founding register.
    #[test]
    fn report_rows_speak_the_founding_register() {
        assert_eq!(
            report("nika.yaml", &Outcome::Created),
            ('✔', "created nika.yaml".to_owned())
        );
        let (mark, msg) = report("nika.yaml", &Outcome::Skipped);
        assert_eq!(mark, '·');
        assert!(msg.contains("--force"), "{msg}");
        let (mark, msg) = report("nika.yaml", &Outcome::Failed("denied".to_owned()));
        assert_eq!(mark, '✖');
        assert!(msg.contains("denied"), "{msg}");
    }
}
