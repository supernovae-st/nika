// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The recipe's ingredients — ONE implementation for every taking door.
//!
//! A taken example must run first try in an empty directory (user
//! gauntlet 2026-07-31 · the one rage-quit: the copied recipe read
//! `examples/fixtures/support-queue.json` and its own header taught an
//! offline rehearsal that died on `NIKA-BUILTIN-READ-001`). The copy
//! door was repaired first; the guided door (`nika new <words>`)
//! then routed MORE people into the same broken socket (the sequence
//! law: a better router multiplies whatever the take delivers). Both
//! doors now share this one materializer — a future door inherits the
//! repair by construction.

use std::path::Path;

/// The `examples/fixtures/<tail>` references a body reads, each tail
/// cut at its first glob star (a `photos/**` read pulls the whole
/// `photos/` dir) and stripped of a trailing `/`.
#[must_use]
pub fn fixture_prefixes(body: &str) -> Vec<String> {
    const MARK: &str = "examples/fixtures/";
    let mut out: Vec<String> = Vec::new();
    for (i, _) in body.match_indices(MARK) {
        let tail: String = body[i + MARK.len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | '*'))
            .collect();
        let cut = tail.split('*').next().unwrap_or("");
        let cut = cut.trim_end_matches('/');
        if !cut.is_empty() && !out.iter().any(|p| p == cut) {
            out.push(cut.to_owned());
        }
    }
    out
}

/// Write the pack fixtures a body reads beside the taken file — under
/// `<dest dir>/examples/fixtures/…`, the exact relative path the yaml
/// names (moving the fixture without the path would relocate the lie,
/// not kill it). Files already yours are never clobbered. Returns
/// `(written, kept)` for the door's own note.
///
/// # Errors
/// An I/O failure creating a fixture directory or writing a fixture
/// file — the door reports it and refuses to pretend the take is whole.
pub fn materialize(body: &str, dest: &Path) -> std::io::Result<(usize, usize)> {
    let prefixes = fixture_prefixes(body);
    if prefixes.is_empty() {
        return Ok((0, 0));
    }
    let base = dest.parent().unwrap_or_else(|| Path::new(""));
    let (mut written, mut kept) = (0, 0);
    for (tail, bytes) in nika_pack::example_fixture_files() {
        let wanted = prefixes
            .iter()
            .any(|p| tail == p || tail.starts_with(&format!("{p}/")));
        if !wanted {
            continue;
        }
        let target = base.join("examples").join("fixtures").join(tail);
        if target.exists() {
            kept += 1;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, bytes)?;
        written += 1;
    }
    Ok((written, kept))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE LAW (RAMS-3 · the sequence law): EVERY catalog entry that
    /// reads a fixture path, taken into an empty directory, has every
    /// fixture it names materialized beside it — a loop over the WHOLE
    /// corpus, so the next added job inherits the pin instead of
    /// re-breaking in silence (Rosa's rage-quit class).
    #[test]
    fn every_fixture_reading_entry_takes_its_ingredients_along() {
        let mut covered = 0;
        for slug in nika_pack::example_slugs() {
            let body = nika_pack::example(&slug).expect("catalog body");
            let prefixes = fixture_prefixes(body);
            if prefixes.is_empty() {
                continue;
            }
            covered += 1;
            let dir = std::env::temp_dir().join(format!(
                "nika-fixtures-law-{}-{}",
                std::process::id(),
                slug.replace('/', "-")
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("tmpdir");
            let dest = dir.join("taken.nika.yaml");
            std::fs::write(&dest, body).expect("take the body");
            let (written, _kept) = materialize(body, &dest).expect("ingredients follow");
            assert!(written > 0, "{slug}: reads {prefixes:?}, wrote nothing");
            for p in &prefixes {
                let on_disk = dir.join("examples").join("fixtures").join(p);
                assert!(
                    on_disk.exists(),
                    "{slug}: `{p}` named by the body is missing on disk"
                );
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
        assert!(
            covered >= 3,
            "the corpus carries fixture-reading jobs; the law must bite: {covered}"
        );
    }

    /// A body with no fixture reads materializes nothing — and a
    /// fixture already yours is counted kept, never clobbered.
    #[test]
    fn no_reads_no_writes_and_yours_stays_yours() {
        let dir = std::env::temp_dir().join(format!("nika-fixtures-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let dest = dir.join("t.nika.yaml");
        assert_eq!(
            materialize("nika: v1\n", &dest).expect("clean"),
            (0, 0),
            "no fixture references → untouched disk"
        );
        // Pick a real fixture tail and pre-claim it with OUR bytes.
        let (tail, _) = nika_pack::example_fixture_files()
            .first()
            .copied()
            .expect("the pack carries fixtures");
        let mine = dir.join("examples").join("fixtures").join(tail);
        std::fs::create_dir_all(mine.parent().expect("parent")).expect("mkdir");
        std::fs::write(&mine, b"mine, not the pack's").expect("pre-claim");
        let body = format!("# reads examples/fixtures/{tail}\n");
        let (written, kept) = materialize(&body, &dest).expect("kept");
        assert_eq!(
            (written, kept),
            (0, 1),
            "already yours → kept, not clobbered"
        );
        assert_eq!(
            std::fs::read(&mine).expect("still mine"),
            b"mine, not the pack's",
            "the pre-claimed bytes survive"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
