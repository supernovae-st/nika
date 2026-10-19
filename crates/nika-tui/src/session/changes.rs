// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a proposal changes in the files it replaces, line by line, against the bytes they hold
//! now (live gauntlet r06 B8, r03: « Inspect the exact changes » printed a 109-line workflow
//! whole, and the person had to find the one line that changed). The Session keeps each
//! replacement's exact bytes and the witness of the bytes it replaces; the file is read again
//! below the root, and a comparison is shown only where it still holds those very bytes. A
//! read for the person's eyes: nothing here writes, consents or runs.

use std::fmt::Write as _;
use std::path::Path;

use nika_session::work;

use super::acquire;

/// The unchanged lines shown around each change.
const CONTEXT: usize = 2;

/// The most cells one comparison computes (lines before by lines after, past the lines they
/// share at either end); a longer one says so and leaves the exact bytes to `/show`.
const CELLS: usize = 4_000_000;

/// The most lines of changes shown for one file; past them a row counts the rest.
const SHOWN: usize = 120;

/// One line of a comparison: kept, removed from the file as it is now, or added by the proposal.
/// A kept or added line carries its line number in the proposal's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit<'a> {
    Kept(&'a str, usize),
    Removed(&'a str),
    Added(&'a str, usize),
}

impl Edit<'_> {
    const fn changed(&self) -> bool {
        !matches!(self, Self::Kept(..))
    }
}

/// What `candidate` changes in each file it replaces, against that file as it is now below
/// `root`: one section per replaced file, in the set's order; `None` when it replaces none.
pub(super) fn against_saved(root: &Path, candidate: &work::Candidate) -> Option<String> {
    let sections: Vec<String> = (candidate.files.iter())
        .filter_map(|file| {
            let replaced = file.replaces.as_ref()?;
            let path = file.path.display().to_string();
            let now = acquire::fetch(root, &path);
            Some(match (now.bytes(), now.witness()) {
                (Some(bytes), Some(witness)) if witness == replaced.0 => {
                    match std::str::from_utf8(bytes) {
                        Ok(before) => compared(&path, replaced.short(), before, &file.content),
                        Err(_) => format!(
                            "`{path}` as it is now is not text: no line comparison · `/show` below prints every byte"
                        ),
                    }
                }
                (Some(_), _) => format!(
                    "`{path}` changed since this proposal was made: no line comparison against the bytes it replaces"
                ),
                _ => format!("`{path}` cannot be read now: no line comparison"),
            })
        })
        .collect();
    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

/// The comparison of `before` (the file at `path` as it is now, witnessed `short`) with
/// `after` (the proposal's bytes): its size, then each change among the lines around it.
fn compared(path: &str, short: &str, before: &str, after: &str) -> String {
    let head = format!("changes in `{path}` against the file as it is now (witnessed {short})");
    let Some(edits) = edits(before, after) else {
        return format!("{head}: too long to compare here · `/show` below prints every byte");
    };
    let added = edits
        .iter()
        .filter(|e| matches!(e, Edit::Added(..)))
        .count();
    let removed = edits
        .iter()
        .filter(|e| matches!(e, Edit::Removed(_)))
        .count();
    let mut out = format!("{head} · +{added} −{removed} lines");
    if added + removed == 0 {
        out.push_str("\n  the same lines, byte for byte");
        return out;
    }
    let mut shown = 0;
    let mut left = 0;
    for hunk in hunks(&edits) {
        let Some(slice) = edits.get(hunk.clone()) else {
            continue;
        };
        if shown >= SHOWN {
            left += slice.iter().filter(|e| e.changed()).count();
            continue;
        }
        let _ = write!(out, "\n  at line {}", line_of(&edits, hunk.start));
        for edit in slice {
            shown += 1;
            match edit {
                Edit::Kept(line, _) => {
                    let _ = write!(out, "\n      {line}");
                }
                Edit::Removed(line) => {
                    let _ = write!(out, "\n    - {line}");
                }
                Edit::Added(line, _) => {
                    let _ = write!(out, "\n    + {line}");
                }
            }
        }
    }
    if left > 0 {
        let _ = write!(
            out,
            "\n  … {left} more changed lines · `/show` below prints every byte"
        );
    }
    out
}

/// The line number in the proposal's bytes a hunk starting at `start` reads at: its first kept
/// or added line, else the line after the last one before it.
fn line_of(edits: &[Edit<'_>], start: usize) -> usize {
    let numbered = |edit: &Edit<'_>| match edit {
        Edit::Kept(_, at) | Edit::Added(_, at) => Some(*at),
        Edit::Removed(_) => None,
    };
    (edits.iter().skip(start).find_map(numbered))
        .or_else(|| {
            edits
                .iter()
                .take(start)
                .rev()
                .find_map(numbered)
                .map(|at| at + 1)
        })
        .unwrap_or(1)
}

/// The ranges of `edits` shown: each change with [`CONTEXT`] kept lines around it, ranges that
/// meet merged.
fn hunks(edits: &[Edit<'_>]) -> Vec<std::ops::Range<usize>> {
    let mut out: Vec<std::ops::Range<usize>> = Vec::new();
    for (at, edit) in edits.iter().enumerate() {
        if !edit.changed() {
            continue;
        }
        let range = at.saturating_sub(CONTEXT)..(at + CONTEXT + 1).min(edits.len());
        match out.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => out.push(range),
        }
    }
    out
}

/// The lines of `after` compared with those of `before`, in order: the longest run of lines
/// both keep, the others removed then added at each place. `None` when the part they do not
/// share at either end is too long to compare here ([`CELLS`]).
fn edits<'a>(before: &'a str, after: &'a str) -> Option<Vec<Edit<'a>>> {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let prefix = (old.iter().zip(&new)).take_while(|(a, b)| a == b).count();
    let suffix = (old.iter().skip(prefix).rev())
        .zip(new.iter().skip(prefix).rev())
        .take_while(|(a, b)| a == b)
        .count();
    let o: Vec<&str> = old[prefix..old.len() - suffix].to_vec();
    let n: Vec<&str> = new[prefix..new.len() - suffix].to_vec();
    if o.len().saturating_mul(n.len()) > CELLS {
        return None;
    }
    // The longest common subsequence of the middle, from its end: `kept[i][j]` lines are kept
    // between `o[i..]` and `n[j..]`.
    let width = n.len() + 1;
    let mut kept = vec![0_u32; (o.len() + 1) * width];
    for i in (0..o.len()).rev() {
        for j in (0..n.len()).rev() {
            kept[i * width + j] = if o[i] == n[j] {
                kept[(i + 1) * width + j + 1] + 1
            } else {
                kept[(i + 1) * width + j].max(kept[i * width + j + 1])
            };
        }
    }
    let mut out: Vec<Edit<'a>> = (old.iter().take(prefix).enumerate())
        .map(|(at, line)| Edit::Kept(line, at + 1))
        .collect();
    let (mut i, mut j) = (0, 0);
    while i < o.len() || j < n.len() {
        if i < o.len() && j < n.len() && o[i] == n[j] {
            out.push(Edit::Kept(o[i], prefix + j + 1));
            i += 1;
            j += 1;
        } else if i < o.len()
            && (j == n.len() || kept[(i + 1) * width + j] >= kept[i * width + j + 1])
        {
            out.push(Edit::Removed(o[i]));
            i += 1;
        } else {
            out.push(Edit::Added(n[j], prefix + j + 1));
            j += 1;
        }
    }
    let tail = new.len() - suffix;
    out.extend(
        (new.iter().skip(tail).enumerate()).map(|(at, line)| Edit::Kept(line, tail + at + 1)),
    );
    Some(out)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;
