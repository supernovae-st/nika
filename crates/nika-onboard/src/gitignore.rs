// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `.gitignore` guarantee — a founded repo cannot commit its own
//! run traces.
//!
//! `.nika/traces/*.ndjson` journals carry model outputs, the file
//! contents tasks read, and tool arguments — the exact payload class a
//! user means to keep local. `cargo new` has laid a `.gitignore` down
//! from day one for `target/`; terraform never did for `*.tfstate`,
//! and thousands of state files with secrets leaked into public repos.
//! Init takes the cargo side, in BOTH lanes (scripted and wizard):
//! ADDS-ONLY — create the file when absent, append one marked section
//! when the human's file lacks the entry, and never overwrite, never
//! reformat, never duplicate (a second `init` writes nothing).

use std::path::Path;

/// The one section, marked — the whole contract in two lines. A fresh
/// file starts on it directly; an existing one gets it behind ONE
/// blank line, the human's bytes leading verbatim.
pub(crate) const SECTION: &str =
    "# nika — run traces carry model outputs, file contents and tool arguments\n.nika/traces/\n";

/// The entry both spellings of the cover share — presence of EITHER
/// (or of the broader `.nika/` rule) means the cover exists, ours or
/// hand-written, and init adds nothing.
const ENTRY: &str = ".nika/traces/";

/// What one `ensure` came to — the lanes compose their own register
/// over it (scripted speaks the joined path · the wizard speaks
/// project-relative).
pub(crate) enum Outcome {
    /// The file was absent — created carrying the section.
    Created,
    /// The file existed without the entry — the section was appended.
    Appended,
    /// The entry was already there (ours or the human's) — nothing
    /// written (the idempotence law: a second init adds nothing).
    Covered,
    /// A read/write failure — init's one environment error (exit 3).
    Failed(String),
}

/// Ensure `dir/.gitignore` ignores the trace dir — adds-only.
pub(crate) fn ensure(dir: &str) -> (String, Outcome) {
    let path = Path::new(dir)
        .join(".gitignore")
        .to_string_lossy()
        .into_owned();
    match ensure_inner(&path) {
        Ok(outcome) => (path, outcome),
        Err(e) => (path, Outcome::Failed(e.to_string())),
    }
}

/// The io half — `?` propagation, the caller owns the error voice.
fn ensure_inner(path: &str) -> std::io::Result<Outcome> {
    let body = match std::fs::read_to_string(path) {
        Ok(body) => Some(body),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    // Same care as the briefs' write: the dir may not exist yet on a
    // bare `init new-dir` — create parents, never fail on the missing.
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    match body {
        None => {
            std::fs::write(path, SECTION)?;
            Ok(Outcome::Created)
        }
        Some(body) if covers(&body) => Ok(Outcome::Covered),
        Some(body) => {
            std::fs::write(path, appended(&body))?;
            Ok(Outcome::Appended)
        }
    }
}

/// The human's bytes verbatim, then the section behind ONE blank line
/// — a missing trailing newline never glues the marker onto their last
/// entry, and an empty file reads as a create (no leading blanks).
fn appended(body: &str) -> String {
    if body.trim().is_empty() {
        return SECTION.to_owned();
    }
    let mut next = String::with_capacity(body.len() + SECTION.len() + 2);
    next.push_str(body);
    if !next.ends_with('\n') {
        next.push('\n');
    }
    next.push('\n');
    next.push_str(SECTION);
    next
}

/// Does the file already ignore the trace dir? The exact entry (either
/// spelling) or the broader `.nika` rule — a hand-written cover counts
/// the same as ours; that is what makes a second run a no-op.
fn covers(body: &str) -> bool {
    body.lines()
        .map(str::trim)
        .any(|l| l == ENTRY || l == ".nika/traces" || l == ".nika/" || l == ".nika")
}

/// The report row for one outcome — the founding register's ✔/·/✖
/// vocabulary, `path` caller-shaped (joined in the scripted lane ·
/// project-relative in the wizard).
#[must_use]
pub(crate) fn report(path: &str, outcome: &Outcome) -> (char, String) {
    match outcome {
        Outcome::Created => ('✔', format!("created {path}")),
        Outcome::Appended => ('✔', format!("updated {path} (.nika/traces/ now ignored)")),
        Outcome::Covered => (
            '·',
            format!("skipped {path} (.nika/traces/ already ignored)"),
        ),
        Outcome::Failed(e) => ('✖', format!("{path}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-gitignore-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    /// An absent file is CREATED carrying the section — the cargo-new
    /// shape: the repo is born unable to commit its traces.
    #[test]
    fn an_absent_file_is_created_with_the_section() {
        let dir = fresh_dir("create");
        let (path, outcome) = ensure(dir.to_str().expect("utf8"));
        assert!(matches!(outcome, Outcome::Created));
        assert!(path.ends_with(".gitignore"), "{path}");
        let body = std::fs::read_to_string(dir.join(".gitignore")).expect("written");
        assert_eq!(body, SECTION, "a fresh file IS the section, no seam");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// An existing file gains the section APPENDED — every human byte
    /// leads verbatim, nothing reordered, nothing reformatted.
    #[test]
    fn an_existing_file_gains_the_section_and_keeps_every_byte() {
        let dir = fresh_dir("append");
        std::fs::write(dir.join(".gitignore"), "target/\n*.log\n").expect("seed");
        let (_, outcome) = ensure(dir.to_str().expect("utf8"));
        assert!(matches!(outcome, Outcome::Appended));
        let body = std::fs::read_to_string(dir.join(".gitignore")).expect("read");
        assert!(
            body.starts_with("target/\n*.log\n"),
            "the human's lines lead untouched: {body}"
        );
        assert_eq!(
            body,
            format!("target/\n*.log\n\n{SECTION}"),
            "one blank line, then the marked section: {body}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The idempotence law: a second init adds NOTHING — byte-compare,
    /// not line-guess.
    #[test]
    fn a_second_run_adds_nothing() {
        let dir = fresh_dir("twice");
        let first = ensure(dir.to_str().expect("utf8"));
        assert!(matches!(first.1, Outcome::Created));
        let before = std::fs::read_to_string(dir.join(".gitignore")).expect("written");
        let second = ensure(dir.to_str().expect("utf8"));
        assert!(
            matches!(second.1, Outcome::Covered),
            "the second run is a calm cover"
        );
        let after = std::fs::read_to_string(dir.join(".gitignore")).expect("read");
        assert_eq!(before, after, "adds-only means byte-identical");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A hand-written cover counts — init must not stack its section on
    /// an entry the human already wrote (either spelling, or the
    /// broader `.nika` rule).
    #[test]
    fn a_hand_written_entry_is_a_cover() {
        for seed in [".nika/traces/\n", ".nika/traces\n", ".nika/\n"] {
            let dir = fresh_dir("hand");
            std::fs::write(dir.join(".gitignore"), seed).expect("seed");
            let (_, outcome) = ensure(dir.to_str().expect("utf8"));
            assert!(
                matches!(outcome, Outcome::Covered),
                "{seed:?} already covers the trace dir"
            );
            assert_eq!(
                std::fs::read_to_string(dir.join(".gitignore")).expect("read"),
                seed,
                "a covered file is never touched"
            );
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// A file with NO trailing newline gets one before the section —
    /// the marker never glues onto the human's last entry.
    #[test]
    fn a_missing_trailing_newline_never_glues_the_marker() {
        let dir = fresh_dir("glue");
        std::fs::write(dir.join(".gitignore"), "target/").expect("seed");
        let (_, outcome) = ensure(dir.to_str().expect("utf8"));
        assert!(matches!(outcome, Outcome::Appended));
        assert_eq!(
            std::fs::read_to_string(dir.join(".gitignore")).expect("read"),
            format!("target/\n\n{SECTION}"),
            "the newline is restored before the seam"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// An empty (or whitespace-only) file reads as a create — no
    /// leading blank lines above the section.
    #[test]
    fn an_empty_file_gets_the_section_alone() {
        let dir = fresh_dir("empty");
        std::fs::write(dir.join(".gitignore"), "").expect("seed");
        let (_, outcome) = ensure(dir.to_str().expect("utf8"));
        assert!(matches!(outcome, Outcome::Appended));
        assert_eq!(
            std::fs::read_to_string(dir.join(".gitignore")).expect("read"),
            SECTION
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The report rows speak the founding register — the cover row is
    /// calm, the append row names what changed.
    #[test]
    fn report_rows_speak_the_founding_register() {
        assert_eq!(
            report(".gitignore", &Outcome::Created),
            ('✔', "created .gitignore".to_owned())
        );
        let (mark, msg) = report(".gitignore", &Outcome::Appended);
        assert_eq!(mark, '✔');
        assert!(msg.contains("now ignored"), "{msg}");
        let (mark, msg) = report(".gitignore", &Outcome::Covered);
        assert_eq!(mark, '·');
        assert!(msg.contains("already ignored"), "{msg}");
        let (mark, msg) = report(".gitignore", &Outcome::Failed("denied".to_owned()));
        assert_eq!(mark, '✖');
        assert!(msg.contains("denied"), "{msg}");
    }
}
