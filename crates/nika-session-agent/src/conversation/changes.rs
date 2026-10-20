// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a proposal changes from the one the person saw, read from their bytes. QA gauntlet q03
//! and q04: « Changed from the proposal you saw: model claude-haiku-5-5 added » and « value
//! `GEMINI_API_KEY` added » were said under the very bytes, and so the same proposal id, because
//! the line came from the values the author bound and not from the workflow.

/// Whether `value` is written in `bytes`, a project path with or without its `./`.
pub(super) fn written(bytes: &str, value: &str) -> bool {
    let bare = value.trim_start_matches("./");
    !bare.is_empty() && (bytes.contains(value) || bytes.contains(bare))
}

/// The lines `after` adds to `before`, and the lines of `before` it removes, each counted once
/// per occurrence: a line moved elsewhere is neither.
pub(super) fn diff<'a>(before: &'a str, after: &'a str) -> (Vec<&'a str>, Vec<&'a str>) {
    let mut removed: Vec<&str> = before.lines().collect();
    let mut added = Vec::new();
    for line in after.lines() {
        match removed.iter().position(|old| *old == line) {
            Some(at) => {
                removed.remove(at);
            }
            None => added.push(line),
        }
    }
    (added, removed)
}

/// The size of a diff, in lines added and removed.
pub(super) fn stat(added: &[&str], removed: &[&str]) -> String {
    format!("+{} −{} lines", added.len(), removed.len())
}

/// The first written line of `lines`, cut to 60 characters, after `what`.
pub(super) fn first(what: &str, lines: &[&str]) -> Option<String> {
    let line = lines
        .iter()
        .map(|line| line.trim())
        .find(|l| !l.is_empty())?;
    let cut: String = line.chars().take(60).collect();
    let more = if cut.len() < line.len() { "…" } else { "" };
    Some(format!("{what} `{cut}{more}`"))
}

#[cfg(test)]
mod tests;
