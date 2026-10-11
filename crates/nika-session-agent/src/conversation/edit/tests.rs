// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

const SOURCE: &str = "tasks:\n  fetch:\n    url: a\n  keep:\n    expression: b\n";

fn pairs(edits: &[(&str, &str)]) -> Vec<(String, String)> {
    (edits.iter())
        .map(|(old, new)| ((*old).to_owned(), (*new).to_owned()))
        .collect()
}

#[test]
fn several_edits_land_as_one_revision_whatever_their_order() {
    let edits = pairs(&[("expression: b", "expression: c"), ("url: a", "url: z")]);
    let want = "tasks:\n  fetch:\n    url: z\n  keep:\n    expression: c\n";
    assert_eq!(edited(SOURCE, &edits), Ok(want.to_owned()));
}

#[test]
fn each_old_passage_is_read_in_the_original_not_in_an_earlier_edit() {
    // The second old passage is what the first edit writes: it is not in the original.
    let edits = pairs(&[("url: a", "url: b2"), ("url: b2", "url: c")]);
    let refused = edited(SOURCE, &edits);
    assert_eq!(
        refused,
        Err("edit 2: `old` must occur exactly once in the candidate; it occurs 0 times".to_owned())
    );
}

#[test]
fn overlapping_edits_are_refused_and_name_both() {
    let edits = pairs(&[("fetch:\n    url: a", "fetch:"), ("url: a\n  keep", "keep")]);
    let refused = edited(SOURCE, &edits);
    assert_eq!(
        refused,
        Err("edits 1 and 2 overlap: each old passage must stand apart".to_owned())
    );
}

#[test]
fn one_edit_keeps_the_one_passage_words() {
    let twice = edited("a a", &pairs(&[("a", "b")]));
    let none = edited(SOURCE, &pairs(&[("", "b")]));
    assert_eq!(
        twice,
        Err("`old` must occur exactly once in the candidate; it occurs 2 times".to_owned())
    );
    assert!(none.is_err(), "an empty old passage replaces nothing");
    assert!(edited(SOURCE, &[]).is_err(), "no edit, no revision");
}
