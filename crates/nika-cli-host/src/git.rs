// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The git-index observation behind doctor's trace-leak signal.
//!
//! `.nika/traces/*.ndjson` journals carry model outputs, the file
//! contents tasks read, and tool arguments. `nika init` now lays the
//! `.gitignore` cover down at founding time; this probe exists for the
//! repos founded BEFORE it did — it counts the journals the CWD's git
//! index ALREADY tracks, so `doctor` can print the untrack remedy. The
//! read is INDEX-ONLY (`git ls-files -z`): no journal byte is ever
//! opened (the same presence-not-value law the key probes obey), and
//! an unobservable surface — no git binary, or the CWD is no repo — is
//! `None`: silence, never a guess.

use std::path::Path;

/// Journals `dir`'s repo tracks under the trace dir — `None` when the
/// observation was impossible (git absent · not a repo · git refused).
// disallowed_types: `std::process::Command` — the kernel ShellExecutor
// seam is async (tokio) and the probe layer is a SYNC one-shot read at
// the CLI surface; the nika-mcp client.rs carve-out class.
#[allow(clippy::disallowed_types)]
pub(crate) fn tracked_trace_journals(dir: &Path) -> Option<usize> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z", "--", nika_dap::store::TRACE_DIR])
        .current_dir(dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    // `-z` terminates each entry with NUL (no quoting, no newline
    // ambiguity) — the count of terminators IS the count of entries.
    // A fold, not filter().count(): naive_bytecount asks for the
    // bytecount crate, and one NUL count buys no dependency.
    Some(
        out.stdout
            .iter()
            .fold(0usize, |n, b| n + usize::from(*b == 0)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // `std::env::temp_dir()` directly (the founding.rs test pattern):
    // CARGO_TARGET_TMPDIR is unset for --lib unit tests, and the
    // target/tmp fallback would nest the fixture INSIDE this repo —
    // where git discovers the parent .git and the no-repo case becomes
    // untestable.
    fn fresh_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-git-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    // disallowed_types: the fixture's whole job is spawning git — the
    // guard/tests.rs --lib carve-out class (the ban guards production
    // effects, not a test arranging the surface under test).
    #[allow(clippy::disallowed_types)]
    fn git(dir: &Path, args: &[&str]) {
        // Hermetic against the HOST's git config: a global excludesFile
        // covering `.nika/traces/` makes a plain `git add` fail (this
        // fixture hit exactly that on a real dev machine) — the repo
        // under test must see a stock git. (`ls-files` reads the index
        // and never consults ignore rules, so the production probe is
        // untouched by this class.)
        let empty = dir.join("empty-gitconfig");
        std::fs::write(&empty, "# fixture: no global config\n").expect("empty config");
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &empty)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?}");
    }

    /// The index speaks, never the worktree: no repo is `None` (the
    /// unobservable); an UNTRACKED journal is `Some(0)`; only a staged
    /// one counts — the leak doctor means.
    #[test]
    fn tracked_trace_journals_reads_the_index_only() {
        let dir = fresh_dir("tracked");
        assert_eq!(
            tracked_trace_journals(&dir),
            None,
            "no repo — the surface is unobservable"
        );
        git(&dir, &["init", "--quiet"]);
        let traces = dir.join(".nika").join("traces");
        std::fs::create_dir_all(&traces).expect("trace dir");
        std::fs::write(traces.join("run.ndjson"), "{}\n").expect("journal");
        assert_eq!(
            tracked_trace_journals(&dir),
            Some(0),
            "on disk but untracked — no leak yet"
        );
        git(&dir, &["add", ".nika/traces/run.ndjson"]);
        assert_eq!(
            tracked_trace_journals(&dir),
            Some(1),
            "the staged journal is the tracked one"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
