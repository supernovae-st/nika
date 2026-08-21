// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `ceiling:` rung of the project file (D-2026-08-11-N5) — the
//! `--max-cost-usd` flag's DEFAULT, resolved once per invocation.
//!
//! The ladder, locked: the per-invocation flag ALWAYS wins · below it
//! the repo's `nika.yaml` `ceiling:` · below that, no budget (the
//! built-in default — an absent file is today's behavior, bit for
//! bit, zero ceremony). A PRESENT file that will not read or parse
//! refuses the run BEFORE any spend, with its line — a typo'd ceiling
//! must never silently no-op (the `policy.toml` closed law,
//! project-side).

use std::path::Path;

use nika_vocab::project::{self, ProjectError};

/// The ladder at an explicit root (the tempdir-injectable half).
pub(super) fn ladder(flag: Option<f64>, start: &Path) -> Result<Option<f64>, ProjectError> {
    let Some((_path, project)) = project::discover(start)? else {
        return Ok(flag);
    };
    Ok(flag.or(project.ceiling))
}

/// The CWD door — discovery walks up from the invocation directory,
/// git-style (the one walk law, shared with the retention + registry
/// seams via [`project::discover`]). An unresolvable CWD degrades to
/// `.` — the same calm fallback the retention ladder takes (a broken
/// CWD must never block a run either).
pub(super) fn from_cwd(flag: Option<f64>) -> Result<Option<f64>, ProjectError> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    ladder(flag, &cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-ceiling-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    /// The ladder, pinned: the flag ALWAYS wins · the file fills a
    /// flag-less invocation · absence is no-budget (today's bytes).
    #[test]
    fn flag_beats_file_beats_default() {
        let dir = fresh_dir("ladder");
        std::fs::write(dir.join("nika.yaml"), "nika: proj\nceiling: 0.50\n").expect("seed");

        assert_eq!(
            ladder(Some(1.25), &dir).expect("ok"),
            Some(1.25),
            "the flag ALWAYS wins — the file's 0.50 never caps an explicit 1.25"
        );
        assert_eq!(
            ladder(None, &dir).expect("ok"),
            Some(0.50),
            "the file fills the flag-less rung"
        );

        let empty = fresh_dir("absent");
        assert_eq!(
            ladder(None, &empty).expect("absence never refuses"),
            None,
            "no file, no budget — the built-in default"
        );
        assert_eq!(
            ladder(Some(2.0), &empty).expect("ok"),
            Some(2.0),
            "the flag alone speaks"
        );
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&empty).ok();
    }

    /// A broken project file refuses CLOSED — even when the flag
    /// would win: the file is read, so its refusal speaks (never a
    /// silent no-op), and the line rides the error.
    #[test]
    fn a_broken_file_refuses_flag_or_no_flag() {
        let dir = fresh_dir("broken");
        std::fs::write(dir.join("nika.yaml"), "nika: proj\nceling: 0.50\n").expect("typo");
        let err = ladder(None, &dir).unwrap_err();
        assert_eq!(
            err.kind(),
            nika_vocab::project::ProjectErrorKind::UnknownKey
        );
        assert_eq!(err.line(), Some(2), "the typo'd line: {err}");
        assert!(
            ladder(Some(9.99), &dir).is_err(),
            "the flag does not pardon a broken file"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
