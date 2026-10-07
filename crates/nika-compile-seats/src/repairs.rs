// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The law that ends the verifier's repairs when no repair count bounds them.

/// Whether a defect set is progress over the sets already repaired from: it names a part never
/// named before, or it narrows the last set (fewer parts, all among the last). Each new part
/// grows a finite set and each narrowing shrinks the last, so the repairs end.
#[must_use]
pub fn progressed(seen: &[Vec<String>], key: &[String]) -> bool {
    let new = key
        .iter()
        .any(|part| !seen.iter().flatten().any(|named| named == part));
    let narrowed = seen
        .last()
        .is_some_and(|last| key.len() < last.len() && key.iter().all(|part| last.contains(part)));
    new || narrowed
}

#[cfg(test)]
mod tests {
    use super::progressed;

    fn set(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| (*p).to_owned()).collect()
    }

    /// A new part or a narrowing is progress; the same set, or a reshuffle of parts already
    /// repaired from, is not.
    #[test]
    fn progress_is_a_new_part_or_a_narrowing() {
        let seen = [set(&["a", "b"])];
        assert!(progressed(&seen, &set(&["c"])));
        assert!(progressed(&seen, &set(&["a"])));
        assert!(!progressed(&seen, &set(&["a", "b"])));
        assert!(!progressed(&[set(&["a"]), set(&["b"])], &set(&["a"])));
    }
}
